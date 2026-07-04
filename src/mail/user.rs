use lettre::{
    Message, Transport,
    message::{Mailbox, MultiPart},
};
use tracing::debug;

use crate::{Mail, Result, entity::users};

pub const HTML_TEMPLATE: &str = include_str!("template.html");

pub fn send_welcome(mail: &Mail, user: &users::Model, link: &str) -> Result<()> {
    let user_name = user.given_name.clone();
    let app_name = mail.from.name.clone().unwrap();
    let message = "Thanks for joining our platform.";
    let link_lbl = "Set Password";
    let subject = format!("Welcome to {app_name}");
    let text =
        format!("Hi {user_name},\n{message}, Set password here: {link}\nCheers,\n{app_name} Team");

    if cfg!(debug_assertions) {
        debug!(text);
        return Ok(());
    }

    let message = Message::builder()
        .from(mail.from.clone())
        .to(Mailbox::new(None, user.email.parse().unwrap()))
        .subject(&subject)
        .multipart(MultiPart::alternative_plain_html(
            text.to_string(),
            HTML_TEMPLATE
                .replace("{user_name}", &user_name)
                .replace("{app_name}", &app_name)
                .replace("{message}", message)
                .replace("{subject}", &subject)
                .replace("{link}", link)
                .replace("{link_lbl}", link_lbl),
        ))?;

    mail.transport.send(&message)?;
    Ok(())
}

pub fn send_reset(mail: &Mail, user: &users::Model, link: &str) -> Result<()> {
    let user_name = user.given_name.clone();
    let app_name = mail.from.name.clone().unwrap();
    let message = "You requested a password reset link.";
    let link_lbl = "Set Password";
    let subject = format!("{app_name} Password Reset");
    let text =
        format!("Hi {user_name},\n{message}, Set password here: {link}\nCheers,\n{app_name} Team");

    if cfg!(debug_assertions) {
        debug!(text);
        return Ok(());
    }

    let message = Message::builder()
        .from(mail.from.clone())
        .to(Mailbox::new(None, user.email.parse().unwrap()))
        .subject(&subject)
        .multipart(MultiPart::alternative_plain_html(
            text.to_string(),
            HTML_TEMPLATE
                .replace("{user_name}", &user_name)
                .replace("{app_name}", &app_name)
                .replace("{message}", message)
                .replace("{subject}", &subject)
                .replace("{link}", link)
                .replace("{link_lbl}", link_lbl),
        ))?;

    mail.transport.send(&message)?;
    Ok(())
}

/// Minimal HTML escape for finder-controlled text (the note) so it can't inject
/// markup into the notification email.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the ping-notification subject + HTML body + plain-text body from the
/// ping's data. Pure (no I/O) so it can be unit-tested.
pub fn ping_email_content(
    tag_name: &str,
    coords: Option<(f64, f64)>,
    note: &str,
) -> (String, String, String) {
    let tag = esc(tag_name);
    let subject = format!("Someone just pinged “{tag_name}”");

    let (mut html, mut plain) = match coords {
        Some((lat, lon)) => {
            let maps = format!("https://maps.google.com/?q={lat},{lon}");
            (
                format!(
                    "Your tag <b>{tag}</b> was just pinged with a location. \
                     <a href=\"{maps}\">See it on the map</a>."
                ),
                format!("Your tag \"{tag_name}\" was just pinged with a location: {maps}"),
            )
        }
        None => (
            format!("Someone left a note on your tag <b>{tag}</b>."),
            format!("Someone left a note on your tag \"{tag_name}\"."),
        ),
    };

    let note = note.trim();
    if !note.is_empty() {
        html.push_str(&format!("<br><br>They wrote: <i>“{}”</i>", esc(note)));
        plain.push_str(&format!("\nThey wrote: \"{note}\""));
    }

    (subject, html, plain)
}

/// Notify a tag's owner that it was pinged (location and/or note). Best-effort;
/// the caller runs this off the request path.
pub fn send_ping_notification(
    mail: &Mail,
    owner: &users::Model,
    tag_name: &str,
    tracker_id: u64,
    coords: Option<(f64, f64)>,
    note: &str,
    spa_url: &str,
) -> Result<()> {
    let user_name = owner.given_name.clone();
    let app_name = mail.from.name.clone().unwrap();
    let link = format!("{spa_url}/trackers?id={tracker_id}");
    let link_lbl = "View in Dracker";
    let (subject, html_msg, plain_msg) = ping_email_content(tag_name, coords, note);

    let text =
        format!("Hi {user_name},\n{plain_msg}\n{link_lbl}: {link}\nCheers,\n{app_name} Team");

    if cfg!(debug_assertions) {
        debug!(text);
        return Ok(());
    }

    let message = Message::builder()
        .from(mail.from.clone())
        .to(Mailbox::new(None, owner.email.parse().unwrap()))
        .subject(&subject)
        .multipart(MultiPart::alternative_plain_html(
            text,
            HTML_TEMPLATE
                .replace("{user_name}", &user_name)
                .replace("{app_name}", &app_name)
                .replace("{message}", &html_msg)
                .replace("{subject}", &subject)
                .replace("{link}", &link)
                .replace("{link_lbl}", link_lbl),
        ))?;

    mail.transport.send(&message)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ping_email_content;

    #[test]
    fn location_ping_has_map_link() {
        let (subject, html, plain) = ping_email_content("Blue Backpack", Some((14.6, 121.0)), "");
        assert!(subject.contains("Blue Backpack"));
        assert!(html.contains("maps.google.com/?q=14.6,121"));
        assert!(plain.contains("maps.google.com/?q=14.6,121"));
        assert!(!html.contains("They wrote"));
    }

    #[test]
    fn note_only_ping_has_no_map_link() {
        let (_s, html, _p) = ping_email_content("Keys", None, "found at cafe");
        assert!(!html.contains("maps.google.com"));
        assert!(html.contains("left a note"));
        assert!(html.contains("found at cafe"));
    }

    #[test]
    fn note_is_html_escaped() {
        let (_s, html, _p) = ping_email_content("x", None, "<script>alert(1)</script>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
