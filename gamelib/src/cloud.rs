//! GameLib accounts, through Supabase: sign up, log in, and share your
//! library and stats with friends who have GameLib accounts.
//!
//! - **Accounts** use Supabase Auth (email and password; each account also
//!   has a unique username that friends find you by).
//! - **Data** goes through Supabase's REST API to the tables made by
//!   `supabase/schema.sql`. Row Level Security there means a library and
//!   stats can only be read by their owner and accepted friends.
//! - **The project's anon key** is needed for every request. It's the
//!   public key Supabase means apps to include (Project Settings → API);
//!   it's entered in Settings → Account, or built in with the
//!   `GAMELIB_SUPABASE_ANON_KEY` environment variable.
//!
//! The login is remembered with Supabase's refresh token, kept in the
//! local database, so you stay logged in between runs.

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::db::Db;
use crate::extras::{GameExtras, Status};
use crate::model::Game;

/// The GameLib Supabase project.
pub const DEFAULT_URL: &str = "https://ynjmutmaslrabyeucvzk.supabase.co";
/// The anon key, if it was built in.
pub const BUILT_IN_ANON_KEY: Option<&str> = option_env!("GAMELIB_SUPABASE_ANON_KEY");

const URL_KEY: &str = "cloud.url";
const ANON_KEY: &str = "cloud.anon_key";
const SESSION_KEY: &str = "cloud.session";
const SHARE_KEY: &str = "cloud.share";

/// Where the project is and its public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub url: String,
    pub anon_key: String,
}

/// A logged-in account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// When the access token runs out (Unix time).
    pub expires_at: i64,
    pub user_id: String,
    pub email: String,
}

/// A GameLib account as others see it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Profile {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub bio: String,
}

impl Profile {
    pub fn name(&self) -> &str {
        if self.display_name.trim().is_empty() {
            &self.username
        } else {
            &self.display_name
        }
    }
}

/// A friendship, from your side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friendship {
    pub id: i64,
    pub other: Profile,
    pub accepted: bool,
    /// You sent it (rather than them).
    pub outgoing: bool,
}

/// One game in a shared library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedGame {
    #[serde(default)]
    pub user_id: String,
    pub game_key: String,
    pub title: String,
    pub source: String,
    pub playtime_secs: i64,
    pub last_played: Option<i64>,
    pub status: Option<String>,
    pub rating: Option<i16>,
    pub favourite: bool,
    #[serde(default)]
    pub genres: Vec<String>,
}

/// Headline stats, as shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SharedStats {
    #[serde(default)]
    pub user_id: String,
    pub total_playtime_secs: i64,
    pub game_count: i32,
    pub played_count: i32,
    pub finished_count: i32,
    pub session_count: i32,
    pub achievements_unlocked: i32,
    pub top_game: Option<String>,
    pub top_game_secs: i64,
    pub last_played_title: Option<String>,
    pub last_played_at: Option<i64>,
}

/// What sending a friend request did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    Sent,
    /// They had already asked you, so now you're friends.
    Accepted,
    AlreadyFriends,
    NotFound,
    Yourself,
}

// --- Checking what's typed ---------------------------------------------------

/// Usernames: 3–24 letters, digits or underscores (as the database
/// checks).
pub fn check_username(name: &str) -> Result<()> {
    let name = name.trim();
    ensure!(
        (3..=24).contains(&name.chars().count()),
        "a username is 3 to 24 characters"
    );
    ensure!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "a username can only have letters, numbers and _"
    );
    Ok(())
}

pub fn check_email(email: &str) -> Result<()> {
    let email = email.trim();
    let ok = email.split_once('@').is_some_and(|(user, domain)| {
        !user.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
    }) && !email.contains(char::is_whitespace);
    ensure!(ok, "that doesn't look like an email address");
    Ok(())
}

pub fn check_password(password: &str) -> Result<()> {
    ensure!(
        password.chars().count() >= 8,
        "use a password of at least 8 characters"
    );
    Ok(())
}

// --- Talking to Supabase -----------------------------------------------------

/// The message in a Supabase error reply (Auth and REST use different
/// shapes).
pub fn error_message(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    for field in ["msg", "error_description", "message", "error"] {
        if let Some(text) = value.get(field).and_then(Value::as_str) {
            return text.to_string();
        }
    }
    body.chars().take(200).collect()
}

/// Make Supabase's messages friendlier where we can.
fn explain(status: u16, message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("invalid login credentials") {
        "The email or password isn't right.".into()
    } else if lower.contains("email not confirmed") {
        "Confirm your email first: Supabase sent you a link.".into()
    } else if lower.contains("user already registered") {
        "There's already an account with that email. Log in instead.".into()
    } else if lower.contains("relation") && lower.contains("does not exist")
        || status == 404 && lower.contains("function")
    {
        "The GameLib tables aren't set up in Supabase yet: run supabase/schema.sql in the SQL editor.".into()
    } else if status == 401 && lower.contains("invalid api key") {
        "Supabase didn't accept the anon key. Check it in Settings → Account.".into()
    } else if status == 429 {
        "Too many tries. Wait a minute and try again.".into()
    } else {
        message.to_string()
    }
}

fn check(response: Response) -> Result<String> {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        bail!("{}", explain(status.as_u16(), &error_message(&body)))
    }
}

#[derive(Deserialize)]
struct AuthReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    expires_at: Option<i64>,
    user: Option<AuthUser>,
    // Sign-up with email confirmation on returns just the user.
    id: Option<String>,
}

#[derive(Deserialize)]
struct AuthUser {
    id: String,
    #[serde(default)]
    email: String,
}

/// What signing up did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignUp {
    /// Signed up and logged in.
    LoggedIn(Session),
    /// Signed up; Supabase emailed a link to confirm before logging in.
    ConfirmEmail,
}

/// Read Supabase Auth's reply into a session (or "confirm your email").
pub fn parse_auth(body: &str, now: i64) -> Result<SignUp> {
    let reply: AuthReply =
        serde_json::from_str(body).context("Supabase sent back something unexpected")?;
    match (reply.access_token, reply.refresh_token, reply.user) {
        (Some(access), Some(refresh), Some(user)) => Ok(SignUp::LoggedIn(Session {
            access_token: access,
            refresh_token: refresh,
            expires_at: reply
                .expires_at
                .unwrap_or_else(|| now + reply.expires_in.unwrap_or(3600)),
            user_id: user.id,
            email: user.email,
        })),
        _ if reply.id.is_some() => Ok(SignUp::ConfirmEmail),
        _ => bail!("Supabase didn't log you in"),
    }
}

pub struct Cloud {
    project: Project,
    client: Client,
}

impl Cloud {
    pub fn new(project: Project) -> Result<Cloud> {
        ensure!(
            !project.anon_key.trim().is_empty(),
            "add the Supabase anon key in Settings → Account first"
        );
        ensure!(
            project.url.starts_with("https://") || project.url.starts_with("http://127.0.0.1"),
            "the Supabase address must start with https://"
        );
        let mut builder = Client::builder()
            .user_agent(concat!("gamelib/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20));
        if project.url.starts_with("http://127.0.0.1") {
            builder = builder.no_proxy(); // a test server on this machine
        }
        Ok(Cloud {
            client: builder.build()?,
            project,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.project.url.trim_end_matches('/'), path)
    }

    fn anon(&self, request: RequestBuilder) -> RequestBuilder {
        request
            .header("apikey", self.project.anon_key.trim())
            .header(
                "Authorization",
                format!("Bearer {}", self.project.anon_key.trim()),
            )
    }

    fn authed(&self, request: RequestBuilder, session: &Session) -> RequestBuilder {
        request
            .header("apikey", self.project.anon_key.trim())
            .header("Authorization", format!("Bearer {}", session.access_token))
    }

    fn send(&self, request: RequestBuilder) -> Result<String> {
        check(
            request
                .send()
                .context("couldn't reach Supabase (check your internet connection)")?,
        )
    }

    pub fn username_available(&self, username: &str) -> Result<bool> {
        let body = self.send(
            self.anon(
                self.client
                    .post(self.url("/rest/v1/rpc/username_available")),
            )
            .json(&json!({"name": username.trim()})),
        )?;
        Ok(serde_json::from_str::<bool>(&body).unwrap_or(false))
    }

    pub fn sign_up(&self, username: &str, email: &str, password: &str, now: i64) -> Result<SignUp> {
        check_username(username)?;
        check_email(email)?;
        check_password(password)?;
        ensure!(
            self.username_available(username)?,
            "the username {} is taken",
            username.trim()
        );
        let body = self.send(
            self.anon(self.client.post(self.url("/auth/v1/signup")))
                .json(&json!({
                    "email": email.trim(),
                    "password": password,
                    "data": {"username": username.trim(), "display_name": username.trim()},
                })),
        )?;
        parse_auth(&body, now)
    }

    pub fn log_in(&self, email: &str, password: &str, now: i64) -> Result<Session> {
        check_email(email)?;
        let body = self.send(
            self.anon(
                self.client
                    .post(self.url("/auth/v1/token?grant_type=password")),
            )
            .json(&json!({"email": email.trim(), "password": password})),
        )?;
        match parse_auth(&body, now)? {
            SignUp::LoggedIn(session) => Ok(session),
            SignUp::ConfirmEmail => bail!("Confirm your email first: Supabase sent you a link."),
        }
    }

    pub fn refresh(&self, session: &Session, now: i64) -> Result<Session> {
        let body = self.send(
            self.anon(
                self.client
                    .post(self.url("/auth/v1/token?grant_type=refresh_token")),
            )
            .json(&json!({"refresh_token": session.refresh_token})),
        )?;
        match parse_auth(&body, now)? {
            SignUp::LoggedIn(session) => Ok(session),
            SignUp::ConfirmEmail => bail!("Supabase didn't renew the login"),
        }
    }

    pub fn log_out(&self, session: &Session) -> Result<()> {
        self.send(self.authed(self.client.post(self.url("/auth/v1/logout")), session))?;
        Ok(())
    }

    pub fn my_profile(&self, session: &Session) -> Result<Profile> {
        let body = self.send(self.authed(
            self.client.get(self.url(&format!(
                "/rest/v1/profiles?id=eq.{}&select=*",
                session.user_id
            ))),
            session,
        ))?;
        let mut list: Vec<Profile> = serde_json::from_str(&body)?;
        list.pop()
            .context("your profile is missing (was supabase/schema.sql run before you signed up?)")
    }

    pub fn update_profile(&self, session: &Session, display_name: &str, bio: &str) -> Result<()> {
        self.send(
            self.authed(
                self.client.patch(self.url(&format!("/rest/v1/profiles?id=eq.{}", session.user_id))),
                session,
            )
            .json(&json!({"display_name": display_name.trim(), "bio": bio.trim(), "updated_at": "now()"})),
        )?;
        Ok(())
    }

    pub fn send_friend_request(&self, session: &Session, username: &str) -> Result<RequestOutcome> {
        let body = self.send(
            self.authed(
                self.client
                    .post(self.url("/rest/v1/rpc/send_friend_request")),
                session,
            )
            .json(&json!({"target": username.trim()})),
        )?;
        Ok(
            match serde_json::from_str::<String>(&body)
                .unwrap_or_default()
                .as_str()
            {
                "sent" => RequestOutcome::Sent,
                "accepted" => RequestOutcome::Accepted,
                "already" => RequestOutcome::AlreadyFriends,
                "self" => RequestOutcome::Yourself,
                _ => RequestOutcome::NotFound,
            },
        )
    }

    pub fn respond(&self, session: &Session, request: i64, accept: bool) -> Result<()> {
        self.send(
            self.authed(
                self.client
                    .post(self.url("/rest/v1/rpc/respond_friend_request")),
                session,
            )
            .json(&json!({"request": request, "accept": accept})),
        )?;
        Ok(())
    }

    pub fn remove_friend(&self, session: &Session, other: &str) -> Result<()> {
        self.send(
            self.authed(
                self.client.post(self.url("/rest/v1/rpc/remove_friend")),
                session,
            )
            .json(&json!({"other": other})),
        )?;
        Ok(())
    }

    pub fn friendships(&self, session: &Session) -> Result<Vec<Friendship>> {
        let body = self.send(
            self.authed(
                self.client
                    .get(self.url("/rest/v1/friendships?select=id,requester,addressee,status")),
                session,
            ),
        )?;
        let rows: Vec<FriendshipRow> = serde_json::from_str(&body)?;
        let others: Vec<String> = rows
            .iter()
            .map(|r| {
                if r.requester == session.user_id {
                    r.addressee.clone()
                } else {
                    r.requester.clone()
                }
            })
            .collect();
        if others.is_empty() {
            return Ok(Vec::new());
        }
        let body = self.send(self.authed(
            self.client.get(self.url(&format!(
                "/rest/v1/profiles?select=*&id=in.({})",
                others.join(",")
            ))),
            session,
        ))?;
        let profiles: Vec<Profile> = serde_json::from_str(&body)?;
        Ok(combine_friendships(&session.user_id, rows, profiles))
    }

    /// Replace your shared library and stats with these.
    pub fn share(
        &self,
        session: &Session,
        games: &[SharedGame],
        stats: &SharedStats,
    ) -> Result<()> {
        let rows: Vec<Value> = games
            .iter()
            .map(|g| {
                let mut v = serde_json::to_value(g).unwrap_or(Value::Null);
                v["user_id"] = json!(session.user_id);
                v
            })
            .collect();
        if !rows.is_empty() {
            self.send(
                self.authed(
                    self.client
                        .post(self.url("/rest/v1/library_games?on_conflict=user_id,game_key")),
                    session,
                )
                .header("Prefer", "resolution=merge-duplicates,return=minimal")
                .json(&rows),
            )?;
        }
        // Games no longer in the library are removed.
        let keep: Vec<String> = games
            .iter()
            .map(|g| format!("\"{}\"", g.game_key.replace('"', "")))
            .collect();
        let filter = if keep.is_empty() {
            format!("user_id=eq.{}", session.user_id)
        } else {
            format!(
                "user_id=eq.{}&game_key=not.in.({})",
                session.user_id,
                keep.join(",")
            )
        };
        self.send(
            self.authed(
                self.client
                    .delete(self.url(&format!("/rest/v1/library_games?{filter}"))),
                session,
            ),
        )?;
        let mut stats = serde_json::to_value(stats)?;
        stats["user_id"] = json!(session.user_id);
        self.send(
            self.authed(
                self.client
                    .post(self.url("/rest/v1/user_stats?on_conflict=user_id")),
                session,
            )
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(&stats),
        )?;
        Ok(())
    }

    /// Stop sharing: delete your library and stats from Supabase.
    pub fn unshare(&self, session: &Session) -> Result<()> {
        self.send(self.authed(
            self.client.delete(self.url(&format!(
                "/rest/v1/library_games?user_id=eq.{}",
                session.user_id
            ))),
            session,
        ))?;
        self.send(self.authed(
            self.client.delete(self.url(&format!(
                "/rest/v1/user_stats?user_id=eq.{}",
                session.user_id
            ))),
            session,
        ))?;
        Ok(())
    }

    /// A friend's library (most played first) and stats.
    pub fn friend_library(
        &self,
        session: &Session,
        user_id: &str,
    ) -> Result<(Vec<SharedGame>, Option<SharedStats>)> {
        let body = self.send(self.authed(
            self.client.get(self.url(&format!(
                "/rest/v1/library_games?user_id=eq.{user_id}&select=*&order=playtime_secs.desc"
            ))),
            session,
        ))?;
        let games: Vec<SharedGame> = serde_json::from_str(&body)?;
        let body = self.send(self.authed(
            self.client.get(self.url(&format!(
                "/rest/v1/user_stats?user_id=eq.{user_id}&select=*"
            ))),
            session,
        ))?;
        let mut stats: Vec<SharedStats> = serde_json::from_str(&body)?;
        Ok((games, stats.pop()))
    }
}

#[derive(Deserialize)]
struct FriendshipRow {
    id: i64,
    requester: String,
    addressee: String,
    status: String,
}

fn combine_friendships(
    me: &str,
    rows: Vec<FriendshipRow>,
    profiles: Vec<Profile>,
) -> Vec<Friendship> {
    let mut out: Vec<Friendship> = rows
        .into_iter()
        .filter_map(|r| {
            let outgoing = r.requester == me;
            let other_id = if outgoing { &r.addressee } else { &r.requester };
            let other = profiles.iter().find(|p| &p.id == other_id)?.clone();
            Some(Friendship {
                id: r.id,
                other,
                accepted: r.status == "accepted",
                outgoing,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.accepted.cmp(&a.accepted).then(
            a.other
                .name()
                .to_lowercase()
                .cmp(&b.other.name().to_lowercase()),
        )
    });
    out
}

// --- What's shared --------------------------------------------------------

/// Your library as it's shared (hidden games are left out).
pub fn shared_games(
    games: &[Game],
    extras: &std::collections::HashMap<i64, GameExtras>,
) -> Vec<SharedGame> {
    games
        .iter()
        .filter(|g| !g.hidden)
        .map(|g| {
            let e = extras.get(&g.id).cloned().unwrap_or_default();
            SharedGame {
                user_id: String::new(),
                game_key: format!("{}:{}", g.source.as_str(), g.source_id),
                title: g.title.clone(),
                source: g.source.as_str().to_string(),
                playtime_secs: g.playtime_secs as i64,
                last_played: g.last_played,
                status: e.status.map(|s| s.as_str().to_string()),
                rating: e.rating.map(i16::from),
                favourite: g.favourite,
                genres: g.genres.clone(),
            }
        })
        .collect()
}

pub fn shared_stats(
    games: &[Game],
    extras: &std::collections::HashMap<i64, GameExtras>,
    sessions: usize,
    achievements: usize,
) -> SharedStats {
    let visible: Vec<&Game> = games.iter().filter(|g| !g.hidden).collect();
    let top = visible
        .iter()
        .filter(|g| g.playtime_secs > 0)
        .max_by_key(|g| g.playtime_secs);
    let last = visible
        .iter()
        .filter(|g| g.last_played.is_some())
        .max_by_key(|g| g.last_played);
    SharedStats {
        user_id: String::new(),
        total_playtime_secs: visible.iter().map(|g| g.playtime_secs as i64).sum(),
        game_count: visible.len() as i32,
        played_count: visible.iter().filter(|g| g.playtime_secs > 0).count() as i32,
        finished_count: visible
            .iter()
            .filter(|g| extras.get(&g.id).and_then(|e| e.status) == Some(Status::Finished))
            .count() as i32,
        session_count: sessions as i32,
        achievements_unlocked: achievements as i32,
        top_game: top.map(|g| g.title.clone()),
        top_game_secs: top.map(|g| g.playtime_secs as i64).unwrap_or(0),
        last_played_title: last.map(|g| g.title.clone()),
        last_played_at: last.and_then(|g| g.last_played),
    }
}

// --- Saved settings -------------------------------------------------------

impl Db {
    /// The Supabase project: the address and anon key from Settings, the
    /// built-in key, or the GAMELIB_SUPABASE_ANON_KEY variable.
    pub fn cloud_project(&self) -> Result<Project> {
        let url = self
            .get_setting(URL_KEY)?
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_string());
        let anon_key = self
            .get_setting(ANON_KEY)?
            .filter(|k| !k.trim().is_empty())
            .or_else(|| BUILT_IN_ANON_KEY.map(str::to_string))
            .or_else(|| std::env::var("GAMELIB_SUPABASE_ANON_KEY").ok())
            .unwrap_or_default();
        Ok(Project { url, anon_key })
    }

    pub fn set_cloud_anon_key(&self, key: &str) -> Result<()> {
        self.set_setting(ANON_KEY, key.trim())
    }

    pub fn cloud_session(&self) -> Result<Option<Session>> {
        Ok(self
            .get_setting(SESSION_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok()))
    }

    pub fn set_cloud_session(&self, session: Option<&Session>) -> Result<()> {
        match session {
            Some(s) => self.set_setting(SESSION_KEY, &serde_json::to_string(s)?),
            None => self.delete_setting(SESSION_KEY),
        }
    }

    /// Share your library and stats with friends (on unless switched off).
    pub fn cloud_sharing(&self) -> Result<bool> {
        self.get_bool_setting(SHARE_KEY, true)
    }

    pub fn set_cloud_sharing(&self, on: bool) -> Result<()> {
        self.set_bool_setting(SHARE_KEY, on)
    }
}

/// A session that's good for at least another minute, renewing it if
/// needed (and saving the renewed one).
pub fn fresh_session(db: &Db, cloud: &Cloud, now: i64) -> Result<Session> {
    let session = db.cloud_session()?.context("you're not logged in")?;
    if session.expires_at - now > 60 {
        return Ok(session);
    }
    match cloud.refresh(&session, now) {
        Ok(renewed) => {
            db.set_cloud_session(Some(&renewed))?;
            Ok(renewed)
        }
        Err(err) => {
            // A refresh token that's been revoked won't work again.
            if !format!("{err:#}").contains("couldn't reach") {
                db.set_cloud_session(None)?;
            }
            Err(err.context("your login ran out; log in again"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DetectedGame, LaunchTarget, Source};

    #[test]
    fn what_is_typed_is_checked() {
        assert!(check_username("morgan_99").is_ok());
        assert!(check_username("ab").is_err());
        assert!(check_username("no spaces").is_err());
        assert!(check_email("me@example.com").is_ok());
        assert!(check_email("me@example").is_err());
        assert!(check_email("@example.com").is_err());
        assert!(check_password("short").is_err());
        assert!(check_password("long enough").is_ok());
    }

    #[test]
    fn auth_replies_become_sessions() {
        let login = r#"{"access_token":"a1","token_type":"bearer","expires_in":3600,"expires_at":1700003600,
            "refresh_token":"r1","user":{"id":"u-1","email":"me@example.com"}}"#;
        assert_eq!(
            parse_auth(login, 1_700_000_000).unwrap(),
            SignUp::LoggedIn(Session {
                access_token: "a1".into(),
                refresh_token: "r1".into(),
                expires_at: 1_700_003_600,
                user_id: "u-1".into(),
                email: "me@example.com".into(),
            })
        );
        let confirm =
            r#"{"id":"u-2","email":"x@example.com","confirmation_sent_at":"2026-01-01T00:00:00Z"}"#;
        assert_eq!(parse_auth(confirm, 0).unwrap(), SignUp::ConfirmEmail);
        assert!(parse_auth("{}", 0).is_err());
    }

    #[test]
    fn errors_are_read_and_explained() {
        assert_eq!(
            explain(
                400,
                &error_message(
                    r#"{"error":"invalid_grant","error_description":"Invalid login credentials"}"#
                )
            ),
            "The email or password isn't right."
        );
        assert_eq!(
            explain(
                400,
                &error_message(
                    r#"{"code":400,"error_code":"invalid_credentials","msg":"Invalid login credentials"}"#
                )
            ),
            "The email or password isn't right."
        );
        assert!(
            explain(
                404,
                &error_message(r#"{"message":"relation \"public.profiles\" does not exist"}"#)
            )
            .contains("schema.sql")
        );
    }

    #[test]
    fn friendships_are_seen_from_your_side() {
        let rows = vec![
            FriendshipRow {
                id: 1,
                requester: "me".into(),
                addressee: "b".into(),
                status: "accepted".into(),
            },
            FriendshipRow {
                id: 2,
                requester: "c".into(),
                addressee: "me".into(),
                status: "pending".into(),
            },
            FriendshipRow {
                id: 3,
                requester: "me".into(),
                addressee: "gone".into(),
                status: "pending".into(),
            },
        ];
        let profile = |id: &str, name: &str| Profile {
            id: id.into(),
            username: name.into(),
            display_name: String::new(),
            bio: String::new(),
        };
        let list = combine_friendships("me", rows, vec![profile("b", "bea"), profile("c", "cal")]);
        assert_eq!(list.len(), 2, "a friend with no profile is left out");
        assert!(list[0].accepted && list[0].outgoing && list[0].other.username == "bea");
        assert!(!list[1].accepted && !list[1].outgoing);
    }

    #[test]
    fn your_library_and_stats_are_summed_up_for_sharing() {
        let db = Db::open_in_memory().unwrap();
        let mut steam = DetectedGame::manual("Hades", "/g/h.exe");
        steam.source = Source::Steam;
        steam.source_id = "1145360".into();
        steam.launch = LaunchTarget::Uri("steam://rungameid/1145360".into());
        let hades = db.upsert_detected(&steam, 1).unwrap().game_id;
        let secret = db
            .upsert_detected(&DetectedGame::manual("Secret", "/g/s.exe"), 1)
            .unwrap()
            .game_id;
        db.set_hidden(secret, true).unwrap();
        db.set_status(hades, Some(Status::Finished)).unwrap();
        let games = db.list_games().unwrap();
        let extras = db.all_extras().unwrap();
        let shared = shared_games(&games, &extras);
        assert_eq!(shared.len(), 1, "hidden games aren't shared");
        assert_eq!(shared[0].game_key, "steam:1145360");
        assert_eq!(shared[0].status.as_deref(), Some("finished"));
        let stats = shared_stats(&games, &extras, 4, 2);
        assert_eq!(
            (stats.game_count, stats.finished_count, stats.session_count),
            (1, 1, 4)
        );
    }

    #[test]
    fn the_session_and_key_are_remembered() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.cloud_project().unwrap().url, DEFAULT_URL);
        db.set_cloud_anon_key("  key123 ").unwrap();
        assert_eq!(db.cloud_project().unwrap().anon_key, "key123");
        let session = Session {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 10_000,
            user_id: "u".into(),
            email: "e@x.io".into(),
        };
        db.set_cloud_session(Some(&session)).unwrap();
        assert_eq!(db.cloud_session().unwrap(), Some(session.clone()));
        let cloud = Cloud::new(db.cloud_project().unwrap()).unwrap();
        // Still fresh: used as it is, no network needed.
        assert_eq!(fresh_session(&db, &cloud, 100).unwrap(), session);
        db.set_cloud_session(None).unwrap();
        assert!(fresh_session(&db, &cloud, 100).is_err());
        assert!(
            Cloud::new(Project {
                url: DEFAULT_URL.into(),
                anon_key: " ".into()
            })
            .is_err()
        );
    }

    /// A pretend Supabase: answers each request from `reply` and records
    /// what was asked.
    fn fake_supabase(
        count: usize,
        reply: impl Fn(&str, &str) -> (u16, String) + Send + 'static,
    ) -> (
        String,
        std::sync::mpsc::Receiver<(String, String, String, String)>,
    ) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..count {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let mut length = 0;
                let mut auth = String::new();
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    let header = header.trim_end().to_string();
                    if header.is_empty() {
                        break;
                    }
                    let lower = header.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = header["authorization:".len()..].trim().to_string();
                    }
                    if lower.starts_with("apikey:") {
                        assert_eq!(header["apikey:".len()..].trim(), "anon-key");
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let (status, answer) = reply(&method, &path);
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                )
                .unwrap();
                tx.send((method, path, auth, body)).unwrap();
            }
        });
        (url, rx)
    }

    #[test]
    fn sign_up_befriend_share_and_look() {
        let (url, seen) = fake_supabase(9, |method, path| {
            let ok = |body: &str| (200, body.to_string());
            match (method, path) {
                ("POST", "/rest/v1/rpc/username_available") => ok("true"),
                ("POST", "/auth/v1/signup") => ok(
                    r#"{"access_token":"tok","refresh_token":"ref","expires_in":3600,"user":{"id":"me-id","email":"me@example.com"}}"#,
                ),
                ("POST", "/rest/v1/rpc/send_friend_request") => ok(r#""sent""#),
                ("POST", p) if p.starts_with("/rest/v1/library_games") => (201, String::new()),
                ("DELETE", p) if p.starts_with("/rest/v1/library_games") => (204, String::new()),
                ("POST", p) if p.starts_with("/rest/v1/user_stats") => (201, String::new()),
                ("GET", p) if p.starts_with("/rest/v1/library_games") => ok(
                    r#"[{"user_id":"bea-id","game_key":"steam:570","title":"Dota 2","source":"steam","playtime_secs":3600,"last_played":null,"status":null,"rating":5,"favourite":true,"genres":["Strategy"]}]"#,
                ),
                ("GET", p) if p.starts_with("/rest/v1/user_stats") => ok(
                    r#"[{"user_id":"bea-id","total_playtime_secs":3600,"game_count":1,"played_count":1,"finished_count":0,"session_count":3,"achievements_unlocked":2,"top_game":"Dota 2","top_game_secs":3600,"last_played_title":null,"last_played_at":null}]"#,
                ),
                ("POST", "/auth/v1/logout") => (204, String::new()),
                _ => (404, r#"{"message":"unexpected"}"#.to_string()),
            }
        });
        let cloud = Cloud::new(Project {
            url,
            anon_key: "anon-key".into(),
        })
        .unwrap();
        let SignUp::LoggedIn(session) = cloud
            .sign_up("morgan", "me@example.com", "password1", 100)
            .unwrap()
        else {
            panic!("expected to be logged in");
        };
        assert_eq!(session.expires_at, 3700);
        assert_eq!(
            cloud.send_friend_request(&session, "bea").unwrap(),
            RequestOutcome::Sent
        );
        let game = SharedGame {
            user_id: String::new(),
            game_key: "steam:1145360".into(),
            title: "Hades".into(),
            source: "steam".into(),
            playtime_secs: 60,
            last_played: None,
            status: None,
            rating: None,
            favourite: false,
            genres: vec![],
        };
        cloud
            .share(&session, &[game], &SharedStats::default())
            .unwrap();
        let (games, stats) = cloud.friend_library(&session, "bea-id").unwrap();
        assert_eq!(games[0].title, "Dota 2");
        assert_eq!(stats.unwrap().session_count, 3);
        cloud.log_out(&session).unwrap();

        let requests: Vec<_> = seen.try_iter().collect();
        assert_eq!(requests.len(), 9);
        // Before logging in, the anon key is the bearer token; after, the
        // session's token is.
        assert_eq!(requests[0].2, "Bearer anon-key");
        assert!(
            requests[1].3.contains(r#""username":"morgan""#),
            "{}",
            requests[1].3
        );
        assert_eq!(requests[2].2, "Bearer tok");
        assert!(requests[2].3.contains(r#""target":"bea""#));
        assert!(requests[3].1.contains("on_conflict=user_id,game_key"));
        assert!(
            requests[3].3.contains(r#""user_id":"me-id""#),
            "rows are stamped with your id"
        );
        assert!(
            requests[4]
                .1
                .contains("game_key=not.in.(%22steam:1145360%22)")
                || requests[4]
                    .1
                    .contains("game_key=not.in.(\"steam:1145360\")"),
            "{}",
            requests[4].1
        );
        assert!(requests[6].1.contains("user_id=eq.bea-id"));
    }
}
