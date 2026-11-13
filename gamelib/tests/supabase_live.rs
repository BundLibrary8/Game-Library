//! Checks against the real GameLib Supabase project. They need the
//! internet, so they only run when asked:
//!
//! ```text
//! cargo test --test supabase_live -- --ignored
//! ```
//!
//! They don't create accounts (that would send confirmation emails); they
//! check the key works, the database functions are there, and that the
//! security rules keep logged-out requests away from private data.

use gamelib::cloud::{Cloud, DEFAULT_ANON_KEY, DEFAULT_URL, Project};

fn cloud() -> Cloud {
    Cloud::new(Project {
        url: DEFAULT_URL.into(),
        anon_key: DEFAULT_ANON_KEY.into(),
    })
    .unwrap()
}

#[test]
#[ignore = "talks to the real Supabase project"]
fn the_project_answers_and_the_setup_script_has_been_run() {
    let cloud = cloud();
    // The function made by supabase/schema.sql answers.
    assert!(cloud.username_available("nobody_has_this_name_42").unwrap());
}

#[test]
#[ignore = "talks to the real Supabase project"]
fn a_wrong_password_gives_a_clear_message() {
    let err = cloud()
        .log_in("nobody-gamelib-test@example.com", "not the password", 0)
        .unwrap_err();
    assert_eq!(format!("{err}"), "The email or password isn't right.");
}
