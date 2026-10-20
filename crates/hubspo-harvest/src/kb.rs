//! HubSpot's public default-property pages
//! (`knowledge.hubspot.com/.../hubspots-default-*-properties`): the labels of
//! the standard properties of one object type, in page order, with the
//! section they appear under.
//!
//! This is the provisional source of D-ONT-6's position tables (operator
//! decision 2026-10-20): the pages give display labels only. They carry no
//! internal property name, no type and no options, and they leave out
//! HubSpot's hidden internal properties, so nothing here may be read as the
//! authoritative property list. That list comes from a portal's properties
//! API (`ISS-ORACLE-PORTAL`).
//!
//! An entry is an item of a top-level bulleted list (`<ul><li>` with no list
//! around the `<ul>`) that starts with a bold label followed by a colon:
//! `<strong>Label</strong>: …`, `<strong>Label:</strong> …`, or the same with
//! `<b>` or `<span style="font-weight: bold">`, optionally inside a paragraph
//! or a link. Not entries: callouts (`<li class="alert">`), numbered steps
//! (`<ol><li>`), nested lists (a property's options, such as a ticket's SLA
//! states), and list items whose bold text comes later. The page's text is
//! never kept: only the label and the heading above it.

/// One property label as the page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbEntry {
    /// The nearest `<h2>`/`<h3>` heading above the entry; empty before the
    /// first heading.
    pub section: String,
    /// The label, whitespace collapsed, trailing colon removed.
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Await {
    /// Not at the start of a candidate list item.
    No,
    /// Just inside a candidate `<li>`: the next content decides.
    First,
    /// Collecting the bold label.
    Label,
    /// After `</strong>`: a colon must follow unless the label ended with one.
    Colon,
    /// Inside a parenthetical between the label and its colon
    /// (`<strong>Label</strong> (Enterprise only): …`).
    Paren,
}

/// Every entry of one page, in page order. Duplicate labels are kept; the
/// caller decides what a repeat means.
#[must_use]
pub fn parse(html: &str) -> Vec<KbEntry> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut heading: Option<String> = None;
    let mut state = Await::No;
    let mut label = String::new();
    // The element that opened the label, so only its own end closes it.
    let mut label_tag = "";
    let mut rest = html;

    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            text(
                rest,
                &mut state,
                &mut label,
                &mut heading,
                &section,
                &mut out,
            );
            break;
        };
        text(
            &rest[..lt],
            &mut state,
            &mut label,
            &mut heading,
            &section,
            &mut out,
        );
        rest = &rest[lt..];
        // Comments and raw-text elements carry no labels.
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |i| &r[i + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let (closing, body) = match tag.strip_prefix('/') {
            Some(b) => (true, b),
            None => (false, tag),
        };
        let name: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if !closing && (name == "script" || name == "style") {
            let end = format!("</{name}");
            rest = find_ci(rest, &end).map_or("", |i| &rest[i..]);
            continue;
        }
        match (closing, name.as_str()) {
            (false, "h2" | "h3") => heading = Some(String::new()),
            (true, "h2" | "h3") => {
                if let Some(h) = heading.take() {
                    section = collapse(&h);
                }
            }
            (false, "ul" | "ol" | "li") => {
                if name == "li" {
                    // An entry's list is a `<ul>` with no list or item around it.
                    let top_level = stack.len() == 1 && stack[0] == "ul";
                    state = if top_level && !body.contains("alert") {
                        Await::First
                    } else {
                        Await::No
                    };
                    label.clear();
                }
                stack.push(name.clone());
            }
            (true, "li") => {
                if let Some(pos) = stack.iter().rposition(|n| n == "li") {
                    stack.truncate(pos);
                }
                state = Await::No;
            }
            (true, "ul" | "ol") => {
                if let Some(pos) = stack.iter().rposition(|n| *n == name) {
                    stack.truncate(pos);
                }
                state = Await::No;
            }
            (false, "p" | "a") if state == Await::First => {}
            (false, "strong" | "b") if state == Await::First => {
                state = Await::Label;
                label_tag = if name == "b" { "b" } else { "strong" };
            }
            (false, "span") if state == Await::First && is_bold(body) => {
                state = Await::Label;
                label_tag = "span";
            }
            (true, t) if state == Await::Label && t == label_tag => {
                let l = collapse(&label);
                if let Some(stripped) = l.strip_suffix(':') {
                    push(&mut out, &section, stripped);
                    state = Await::No;
                } else {
                    state = Await::Colon;
                }
            }
            // A second bold run right after the label continues it
            // (`<strong>C</strong><strong>all direction</strong>`, or the
            // colon in its own `<strong>:</strong>`).
            (false, "strong" | "b") if state == Await::Colon => {
                state = Await::Label;
                label_tag = if name == "b" { "b" } else { "strong" };
            }
            (false, "span") if state == Await::Colon && is_bold(body) => {
                state = Await::Label;
                label_tag = "span";
            }
            // Between the label and its colon, links and inline markup are
            // transparent (`</a>`, `<em>:</em>`, `<span>:</span>`).
            (_, "a" | "em" | "i" | "span" | "strong" | "b")
                if state == Await::Colon || state == Await::Paren => {}
            (_, _) if state == Await::Paren => {}
            // Inline markup inside the label (`<em>`, `<span>`) is transparent.
            (_, _) if state == Await::Label => {}
            // Any other element before the label means this is not an entry.
            (_, _) if state == Await::First || state == Await::Colon => state = Await::No,
            _ => {}
        }
    }
    out
}

fn text(
    t: &str,
    state: &mut Await,
    label: &mut String,
    heading: &mut Option<String>,
    section: &str,
    out: &mut Vec<KbEntry>,
) {
    if let Some(h) = heading.as_mut() {
        h.push_str(t);
    }
    match *state {
        Await::Label => label.push_str(t),
        Await::First if !decode(t).trim().is_empty() => *state = Await::No,
        // Whitespace between two bold runs is a space in the label
        // (`<strong>Industry</strong> <strong>group</strong>`).
        Await::Colon if !t.is_empty() && decode(t).trim().is_empty() => label.push(' '),
        Await::Colon => after_label(&decode(t), state, label, section, out),
        Await::Paren => {
            let d = decode(t);
            if let Some(i) = d.find(')') {
                *state = Await::Colon;
                after_label(&d[i + 1..], state, label, section, out);
            }
        }
        _ => {}
    }
}

/// The text after a label: a colon makes it an entry, a parenthetical defers
/// the decision to after its `)`, anything else ends the candidate.
fn after_label(d: &str, state: &mut Await, label: &str, section: &str, out: &mut Vec<KbEntry>) {
    let d = d.trim_start();
    if d.is_empty() {
        return;
    }
    if d.starts_with(':') {
        push(out, section, &collapse(label));
        *state = Await::No;
    } else if let Some(inner) = d.strip_prefix('(') {
        *state = Await::Paren;
        if let Some(i) = inner.find(')') {
            *state = Await::Colon;
            after_label(&inner[i + 1..], state, label, section, out);
        }
    } else {
        *state = Await::No;
    }
}

fn is_bold(attrs: &str) -> bool {
    let a: String = attrs.chars().filter(|c| !c.is_whitespace()).collect();
    a.to_ascii_lowercase().contains("font-weight:bold")
}

fn push(out: &mut Vec<KbEntry>, section: &str, label: &str) {
    let label = label.trim();
    if !label.is_empty() {
        out.push(KbEntry {
            section: section.to_string(),
            label: label.to_string(),
        });
    }
}

/// Decode the entities these pages use and collapse whitespace.
fn collapse(s: &str) -> String {
    decode(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(html: &str) -> Vec<(String, String)> {
        parse(html)
            .into_iter()
            .map(|e| (e.section, e.label))
            .collect()
    }

    fn pair(s: &str, l: &str) -> (String, String) {
        (s.to_string(), l.to_string())
    }

    #[test]
    fn both_colon_placements_are_entries_in_page_order() {
        let html = "<h2>Contact information</h2><ul>\
            <li><strong>First name</strong>: the first name.</li>\
            <li><p><strong>Job title:</strong> the job title.</p></li>\
            <li><strong>Annual&nbsp;revenue</strong> : the revenue.</li></ul>";
        assert_eq!(
            labels(html),
            vec![
                pair("Contact information", "First name"),
                pair("Contact information", "Job title"),
                pair("Contact information", "Annual revenue"),
            ]
        );
    }

    #[test]
    fn callouts_steps_and_late_bold_are_not_entries() {
        let html = "<h2>Ticket</h2><ul>\
            <li><strong>Category</strong>: the category.</li>\
            <li>Use <strong>Settings</strong>: then save.</li>\
            <li><strong>Bold without colon</strong> then text.</li></ul>\
            <ol><li class=\"alert\"><p><strong>Please note:</strong> off.</p>\
            <ol><li>Click <strong>Settings</strong>.</li></ol></li></ol>\
            <ol><li><strong>Step one:</strong> do it.</li></ol>";
        assert_eq!(labels(html), vec![pair("Ticket", "Category")]);
    }

    #[test]
    fn an_alert_inside_a_bulleted_list_is_skipped() {
        let html = "<ul><li class=\"alert\"><strong>Please note:</strong> x</li>\
            <li><strong>Name</strong>: y</li></ul>";
        assert_eq!(labels(html), vec![pair("", "Name")]);
    }

    #[test]
    fn sections_follow_headings_and_scripts_are_ignored() {
        let html = "<script>var s = '<ul><li><strong>Fake</strong>: x</li></ul>';</script>\
            <h2>Deal <em>information</em></h2><ul><li><strong>Amount</strong>: a</li></ul>\
            <!-- <ul><li><strong>Hidden</strong>: x</li></ul> -->\
            <h3>Pipeline</h3><ul><li><strong>Deal stage</strong>: s</li></ul>";
        assert_eq!(
            labels(html),
            vec![
                pair("Deal information", "Amount"),
                pair("Pipeline", "Deal stage")
            ]
        );
    }

    #[test]
    fn bold_spans_and_linked_labels_are_entries() {
        let html = "<ul>\
            <li><span style=\"font-weight: bold;\">Time to first rep assignment:</span> t</li>\
            <li><a href=\"/x\"><strong>Shared users</strong></a>: u</li>\
            <li><span style=\"font-weight: normal;\">Not bold:</span> n</li></ul>";
        assert_eq!(
            labels(html),
            vec![
                pair("", "Time to first rep assignment"),
                pair("", "Shared users")
            ]
        );
    }

    #[test]
    fn a_propertys_option_list_is_not_entries() {
        // A ticket's SLA states are options of one property, listed as a
        // nested list inside its item, or as a list nested directly in a list.
        let html = "<ul><li><strong>SLA status:</strong> options: \
            <ul><li><strong>Active SLA:</strong> a</li></ul></li></ul>\
            <ul><ul><li><strong>Due soon:</strong> d</li></ul></ul>\
            <ul><li><strong>Source</strong>: s</li></ul>";
        assert_eq!(
            labels(html),
            vec![pair("", "SLA status"), pair("", "Source")]
        );
    }

    #[test]
    fn colon_and_label_split_across_runs_still_make_one_entry() {
        let html = "<ul>\
            <li><strong>Fax number</strong><strong>:</strong> f</li>\
            <li><strong>C</strong><strong>all direction</strong>: d</li>\
            <li><strong><span>Industry</span></strong><span> <strong>group</strong>: g</span></li>\
            <li><strong>Call status</strong><em>:&nbsp;</em>s</li>\
            <li><a href=\"/t\"><strong>Traffic drill-down 1</strong></a><span style=\"font-weight: normal;\">:</span> t</li>\
            <li><a href=\"/o\"><strong>Opted out of email: [email type name]</strong></a><strong>:&nbsp;</strong>o</li></ul>";
        assert_eq!(
            labels(html),
            vec![
                pair("", "Fax number"),
                pair("", "Call direction"),
                pair("", "Industry group"),
                pair("", "Call status"),
                pair("", "Traffic drill-down 1"),
                pair("", "Opted out of email: [email type name]"),
            ]
        );
    }

    #[test]
    fn a_parenthetical_may_sit_between_label_and_colon() {
        let html = "<ul>\
            <li><a href=\"/p\"><strong>Contact priority</strong></a>&nbsp;(<em><strong>Marketing Hub</strong> Enterprise</em> only): p</li>\
            <li><strong>Activity assigned to</strong><span style=\"font-weight: normal;\"> (for tasks, <span>Assigned to</span>)</span>: a</li>\
            <li><strong>No colon</strong> (note) then text</li></ul>";
        assert_eq!(
            labels(html),
            vec![
                pair("", "Contact priority"),
                pair("", "Activity assigned to")
            ]
        );
    }

    #[test]
    fn inline_markup_inside_a_label_is_kept_as_text() {
        let html = "<ul><li><strong>Last <em>NPS</em> survey date</strong>: d</li></ul>";
        assert_eq!(labels(html), vec![pair("", "Last NPS survey date")]);
    }
}
