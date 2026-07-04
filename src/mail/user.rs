use chrono::Utc;
use lettre::{
    Message, Transport,
    message::{Mailbox, MultiPart},
};
use tracing::debug;

use crate::{Mail, Result, entity::users};

pub const HTML_TEMPLATE: &str = include_str!("template.html");

/// Minimal HTML escape for user-controlled text so it can't inject markup.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fill the shared shell (header/button/footer). `content` and `below` are
/// trusted HTML built by the senders; user-supplied text inside them must
/// already be escaped.
#[allow(clippy::too_many_arguments)]
fn render(
    app_name: &str,
    subject: &str,
    preheader: &str,
    content: &str,
    link: &str,
    link_lbl: &str,
    below: &str,
    footer_note: &str,
) -> String {
    HTML_TEMPLATE
        .replace("{app_name}", app_name)
        .replace("{subject}", subject)
        .replace("{preheader}", preheader)
        .replace("{content}", content)
        .replace("{link}", link)
        .replace("{link_lbl}", link_lbl)
        .replace("{below}", below)
        .replace("{footer_note}", footer_note)
        .replace("{year}", &Utc::now().format("%Y").to_string())
}

fn deliver(mail: &Mail, to: &str, subject: &str, text: String, html: String) -> Result<()> {
    if cfg!(debug_assertions) {
        debug!(text);
        return Ok(());
    }

    let message = Message::builder()
        .from(mail.from.clone())
        .to(Mailbox::new(None, to.parse().unwrap()))
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(text, html))?;

    mail.transport.send(&message)?;
    Ok(())
}

pub fn send_welcome(mail: &Mail, user: &users::Model, link: &str) -> Result<()> {
    let app_name = mail.from.name.clone().unwrap();
    let user_name = esc(&user.given_name);
    let subject = format!("Welcome to {app_name}");
    let message = "Thanks for joining. Set your password to activate your account.";
    let content = format!(
        "<p style=\"margin:0 0 12px\">Hi {user_name},</p><p style=\"margin:0\">{message}</p>"
    );
    let footer = "You're receiving this because an account was created with this address.";
    let text = format!(
        "Hi {},\n{message}\nSet password: {link}\nCheers,\n{app_name} Team",
        user.given_name
    );

    let html = render(
        &app_name,
        &subject,
        message,
        &content,
        link,
        "Set password",
        "",
        footer,
    );
    deliver(mail, &user.email, &subject, text, html)
}

pub fn send_reset(mail: &Mail, user: &users::Model, link: &str) -> Result<()> {
    let app_name = mail.from.name.clone().unwrap();
    let user_name = esc(&user.given_name);
    let subject = format!("{app_name} password reset");
    let message =
        "You requested a password reset link. If this wasn't you, you can ignore this email.";
    let content = format!(
        "<p style=\"margin:0 0 12px\">Hi {user_name},</p><p style=\"margin:0\">{message}</p>"
    );
    let footer = "You're receiving this because a password reset was requested for this address.";
    let text = format!(
        "Hi {},\n{message}\nSet a new password: {link}\nCheers,\n{app_name} Team",
        user.given_name
    );

    let html = render(
        &app_name,
        &subject,
        message,
        &content,
        link,
        "Set new password",
        "",
        footer,
    );
    deliver(mail, &user.email, &subject, text, html)
}

/// Everything needed to render + send a ping notification. Pure output of
/// `ping_email_content` so it can be unit-tested without SMTP.
pub struct PingEmail {
    pub subject: String,
    pub preheader: String,
    pub content: String,
    pub link: String,
    pub link_lbl: &'static str,
    pub below: String,
    pub text: String,
}

/// Build the ping notification. The map is the money action, so with
/// coordinates the primary button opens the map and the app link goes below;
/// note-only pings make the app the primary.
pub fn ping_email_content(
    tag_name: &str,
    coords: Option<(f64, f64)>,
    note: &str,
    app_link: &str,
) -> PingEmail {
    let tag = esc(tag_name);
    let note = note.trim();
    let when = Utc::now().format("%-d %b %Y, %H:%M UTC").to_string();

    let subject = if note.is_empty() {
        format!("Someone just pinged “{tag_name}”")
    } else {
        format!("Someone pinged “{tag_name}” — and left a note")
    };

    let mut content = match coords {
        Some(_) => format!(
            "<p style=\"margin:0\">Your tag <b>{tag}</b> was just pinged with a location.</p>"
        ),
        None => format!("<p style=\"margin:0\">Someone left a note on your tag <b>{tag}</b>.</p>"),
    };
    content.push_str(&format!(
        "<p style=\"margin:8px 0 0;font-size:13px;color:#8b93a5\">Pinged {when}</p>"
    ));
    if !note.is_empty() {
        content.push_str(&format!(
            "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" style=\"margin:16px 0 0\"><tr>\
             <td style=\"border-left:3px solid #5c9bfb;background-color:#f0f4fd;padding:10px 14px;font-style:italic;color:#1b2235\">“{}”</td>\
             </tr></table>",
            esc(note)
        ));
    }

    let (link, link_lbl, below) = match coords {
        Some((lat, lon)) => (
            format!("https://maps.google.com/?q={lat},{lon}"),
            "See where it was pinged",
            format!(
                "<p style=\"margin:6px 0 0;font-size:13px;color:#8b93a5\">Coordinates: {lat}, {lon}</p>\
                 <p style=\"margin:12px 0 0;font-size:13.5px\"><a href=\"{app_link}\" style=\"color:#5c9bfb\">View in Dracker</a></p>"
            ),
        ),
        None => (app_link.to_string(), "View in Dracker", String::new()),
    };

    let mut text = match coords {
        Some((lat, lon)) => format!(
            "Your tag \"{tag_name}\" was just pinged with a location.\nPinged {when}\nMap: {link}\nCoordinates: {lat}, {lon}"
        ),
        None => format!("Someone left a note on your tag \"{tag_name}\".\nPinged {when}"),
    };
    if !note.is_empty() {
        text.push_str(&format!("\nThey wrote: \"{note}\""));
    }
    text.push_str(&format!("\nView in Dracker: {app_link}"));

    let preheader = if note.is_empty() {
        "Tap to see where it was pinged.".to_string()
    } else {
        format!("They wrote: “{}”", esc(note))
    };

    PingEmail {
        subject,
        preheader,
        content,
        link,
        link_lbl,
        below,
        text,
    }
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
    let app_name = mail.from.name.clone().unwrap();
    let app_link = format!("{spa_url}/trackers?id={tracker_id}");
    let e = ping_email_content(tag_name, coords, note, &app_link);
    let footer = "You're receiving this because your Dracker tag was pinged.";

    let html = render(
        &app_name,
        &e.subject,
        &e.preheader,
        &e.content,
        &e.link,
        e.link_lbl,
        &e.below,
        footer,
    );
    deliver(mail, &owner.email, &e.subject, e.text, html)
}

#[cfg(test)]
mod tests {
    use super::ping_email_content;

    const APP: &str = "https://app.dracker.sh/trackers?id=1";

    #[test]
    fn location_ping_makes_the_map_primary() {
        let e = ping_email_content("Blue Backpack", Some((14.6, 121.0)), "", APP);
        assert!(e.subject.contains("Blue Backpack"));
        assert_eq!(e.link, "https://maps.google.com/?q=14.6,121");
        assert_eq!(e.link_lbl, "See where it was pinged");
        // coords as copyable text + app as secondary link
        assert!(e.below.contains("14.6, 121"));
        assert!(e.below.contains(APP));
        assert!(e.text.contains("maps.google.com/?q=14.6,121"));
        assert!(!e.content.contains("They wrote"));
    }

    #[test]
    fn note_only_ping_makes_the_app_primary() {
        let e = ping_email_content("Keys", None, "found at cafe", APP);
        assert_eq!(e.link, APP);
        assert_eq!(e.link_lbl, "View in Dracker");
        assert!(e.below.is_empty());
        assert!(e.content.contains("left a note"));
        assert!(e.content.contains("found at cafe"));
        assert!(e.preheader.contains("found at cafe"));
    }

    #[test]
    fn note_is_html_escaped() {
        let e = ping_email_content("x", None, "<script>alert(1)</script>", APP);
        assert!(!e.content.contains("<script>"));
        assert!(e.content.contains("&lt;script&gt;"));
        assert!(!e.preheader.contains("<script>"));
    }

    #[test]
    fn subject_hints_at_a_note() {
        let with = ping_email_content("Keys", Some((1.0, 2.0)), "hi", APP);
        let without = ping_email_content("Keys", Some((1.0, 2.0)), "", APP);
        assert!(with.subject.contains("left a note"));
        assert!(!without.subject.contains("left a note"));
    }
}
