//! The admin panel's back end: its password, giving Stardust, and a
//! picture of your own as the background.
//!
//! The password lives in its own plain text file,
//! `admin_password.txt` in GameLib's data folder (on Windows
//! `%APPDATA%\gamelib\admin_password.txt`). Open it in Notepad, change the
//! password line and save; the next unlock uses the new one. If the file
//! is missing it is created with the password `admin`.
//!
//! This keeps casual hands off the panel; it isn't strong security. Anyone
//! who can open the file can read the password.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use rusqlite::params;

use crate::db::Db;

pub const PASSWORD_FILE: &str = "admin_password.txt";
/// The password a new password file starts with.
pub const DEFAULT_PASSWORD: &str = "admin";
/// The ledger reason for Stardust given or taken in the admin panel.
pub const ADMIN_REASON: &str = "admin";
const BACKGROUND_KEY: &str = "custom_background";

const TEMPLATE_HEADER: &str = "\
# GameLib admin panel password.
# Change the line below to your own password and save this file.
# Lines starting with # are ignored. Spaces at either end are ignored.
";

/// Where the password file is.
pub fn password_file() -> Result<PathBuf> {
    Ok(crate::paths::data_dir()?.join(PASSWORD_FILE))
}

/// Create the password file with the default password if it isn't there.
/// Returns true if it was created.
pub fn ensure_password_file(file: &Path) -> Result<bool> {
    if file.exists() {
        return Ok(false);
    }
    std::fs::write(file, format!("{TEMPLATE_HEADER}{DEFAULT_PASSWORD}\n"))
        .with_context(|| format!("couldn't create {}", file.display()))?;
    Ok(true)
}

/// The password: the first line that isn't blank or a `#` comment.
pub fn read_password(file: &Path) -> Result<String> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("couldn't read {}", file.display()))?;
    match text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
    {
        Some(password) => Ok(password.to_string()),
        None => bail!(
            "{} has no password in it. Add one on its own line.",
            file.display()
        ),
    }
}

/// Is `attempt` the password in the file? (Creates the file first if it's
/// missing.) Compared without stopping at the first wrong letter.
pub fn check_password(file: &Path, attempt: &str) -> Result<bool> {
    ensure_password_file(file)?;
    let password = read_password(file)?;
    let (a, b) = (password.as_bytes(), attempt.trim().as_bytes());
    let mut differs = a.len() != b.len();
    for i in 0..a.len().max(b.len()) {
        differs |= a.get(i) != b.get(i);
    }
    Ok(!differs)
}

/// The picture formats a background can be.
pub const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

impl Db {
    /// Give Stardust (or take it away with a negative amount). The balance
    /// can't go below zero. Returns the new balance.
    pub fn admin_give_stardust(&self, amount: i64, now: i64) -> Result<i64> {
        ensure!(amount != 0, "enter an amount other than 0");
        ensure!(
            amount.abs() <= 10_000_000,
            "that's more than 10 million at once"
        );
        let balance = self.stardust_balance()?;
        ensure!(
            balance + amount >= 0,
            "that would leave {} Stardust; the balance can't go below 0",
            balance + amount
        );
        self.conn.execute(
            "INSERT INTO stardust_ledger(amount, reason, created_at) VALUES (?1, ?2, ?3)",
            params![amount, ADMIN_REASON, now],
        )?;
        self.stardust_balance()
    }

    /// The picture chosen as the background, if any (and if it's still there).
    pub fn custom_background(&self) -> Result<Option<PathBuf>> {
        Ok(self
            .get_setting(BACKGROUND_KEY)?
            .map(PathBuf::from)
            .filter(|path| path.is_file()))
    }

    /// Copy a picture into `dir` and use it as the background. The copy
    /// means it keeps working if the original is moved or deleted.
    pub fn set_custom_background(&self, picture: &Path, dir: &Path, now: i64) -> Result<PathBuf> {
        let extension = picture
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        ensure!(
            IMAGE_EXTENSIONS.contains(&extension.as_str()),
            "choose a PNG, JPEG or WebP picture"
        );
        image::image_dimensions(picture)
            .with_context(|| format!("{} isn't a picture GameLib can read", picture.display()))?;
        std::fs::create_dir_all(dir)?;
        let copy = dir.join(format!("background-{now}.{extension}"));
        std::fs::copy(picture, &copy)
            .with_context(|| format!("couldn't copy {}", picture.display()))?;
        let old = self.custom_background()?;
        self.set_setting(BACKGROUND_KEY, &copy.to_string_lossy())?;
        if let Some(old) = old.filter(|old| old.starts_with(dir) && *old != copy) {
            let _ = std::fs::remove_file(old);
        }
        Ok(copy)
    }

    /// Go back to the normal background.
    pub fn clear_custom_background(&self) -> Result<()> {
        if let Some(old) = self.custom_background()? {
            let _ = std::fs::remove_file(old);
        }
        self.delete_setting(BACKGROUND_KEY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_file_is_made_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(PASSWORD_FILE);
        assert!(check_password(&file, "admin").unwrap(), "default password");
        assert!(!check_password(&file, "Admin").unwrap());
        assert!(!check_password(&file, "").unwrap());

        std::fs::write(&file, "# a comment\n\n   hunter2  \nignored\n").unwrap();
        assert_eq!(read_password(&file).unwrap(), "hunter2");
        assert!(check_password(&file, "hunter2").unwrap());
        assert!(!check_password(&file, "admin").unwrap());
        assert!(!check_password(&file, "hunter").unwrap());

        std::fs::write(&file, "# only comments\n").unwrap();
        assert!(check_password(&file, "anything").is_err());
    }

    #[test]
    fn stardust_can_be_given_and_taken_but_not_below_zero() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.admin_give_stardust(500, 1).unwrap(), 500);
        assert_eq!(db.admin_give_stardust(-200, 2).unwrap(), 300);
        assert!(db.admin_give_stardust(-301, 3).is_err());
        assert!(db.admin_give_stardust(0, 3).is_err());
        assert_eq!(db.stardust_ledger().unwrap()[0].reason, ADMIN_REASON);
    }

    #[test]
    fn a_background_is_copied_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let picture = dir.path().join("me.png");
        image::RgbImage::new(4, 3).save(&picture).unwrap();
        let store = dir.path().join("backgrounds");
        let db = Db::open_in_memory().unwrap();

        let first = db.set_custom_background(&picture, &store, 1).unwrap();
        assert_eq!(db.custom_background().unwrap(), Some(first.clone()));
        std::fs::remove_file(&picture).unwrap();
        assert!(first.is_file(), "the copy outlives the original");

        let not_a_picture = dir.path().join("notes.png");
        std::fs::write(&not_a_picture, "hello").unwrap();
        assert!(db.set_custom_background(&not_a_picture, &store, 2).is_err());
        assert!(
            db.set_custom_background(&dir.path().join("x.gif"), &store, 2)
                .is_err()
        );

        image::RgbImage::new(2, 2).save(&picture).unwrap();
        let second = db.set_custom_background(&picture, &store, 3).unwrap();
        assert!(!first.exists(), "the old copy is tidied away");
        db.clear_custom_background().unwrap();
        assert_eq!(db.custom_background().unwrap(), None);
        assert!(!second.exists());
    }
}
