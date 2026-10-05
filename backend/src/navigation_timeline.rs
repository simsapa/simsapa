//! The Timeline page of the Sutta Navigation window: every node that has a
//! context note, grouped by the period its note is dated to, as HTML.
//!
//! No Qt and no HTTP. The dates come from the context notes
//! ([`crate::navigation_texts`]), the titles and the order from
//! `navigation_nodes`. The clustering, the ordering and the period text are all
//! decided here; the page's script only lays the result out and draws the axis.
//! The markup of [`render_timeline_html`] is the contract with that script.
//!
//! See docs/sutta-navigation-tree.md.

use std::collections::HashMap;

use diesel::prelude::*;

use crate::db::appdata_schema::navigation_nodes;
use crate::logger::warn;
use crate::navigation_texts::{navigation_texts, NavigationTexts};
use crate::navigation_tree::{with_appdata, NavigationNode};

/// One node with a context note. A year is a plain number, BCE negative.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    pub node_uid: String,
    pub title_pali: Option<String>,
    pub title_en: Option<String>,
    pub note: String,
    pub from: Option<i32>,
    pub to: Option<i32>,
    pub circa: bool,
    /// The node's position in the navigation tree.
    pub sort_path: String,
}

impl TimelineEntry {
    /// The title the navigation row shows on its first line: the Pāli title,
    /// else the English one, else the uid.
    pub fn title(&self) -> &str {
        non_empty(&self.title_pali)
            .or_else(|| non_empty(&self.title_en))
            .unwrap_or(&self.node_uid)
    }

    /// The English title the navigation row shows under the first line: none
    /// when it is already the first line.
    pub fn subtitle(&self) -> Option<&str> {
        non_empty(&self.title_en).filter(|title_en| *title_en != self.title())
    }

    fn tree_order(&self) -> (&str, &str) {
        (self.sort_path.as_str(), self.node_uid.as_str())
    }
}

fn non_empty(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|s| !s.is_empty())
}

/// The entries that share one period: the same `from`, `to` and `circa`. At
/// least one bound is present, and `entries` is never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineCluster {
    pub from: Option<i32>,
    pub to: Option<i32>,
    pub circa: bool,
    pub entries: Vec<TimelineEntry>,
}

impl TimelineCluster {
    /// The year the cluster is placed at: `from`, or `to` when that is the only
    /// bound.
    pub fn sort_year(&self) -> i32 {
        self.from.or(self.to).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Timeline {
    /// In chronological order.
    pub clusters: Vec<TimelineCluster>,
    /// The entries with no bound at all, in tree order.
    pub undated: Vec<TimelineEntry>,
}

impl Timeline {
    pub fn is_empty(&self) -> bool {
        self.clusters.is_empty() && self.undated.is_empty()
    }
}

fn year_text(year: i32) -> String {
    if year < 0 {
        format!("{} BCE", -year)
    } else {
        format!("{} CE", year)
    }
}

/// The period as a reader sees it: `500–200 BCE`, `100 BCE – 200 CE`,
/// `after 300 BCE`, `before 100 BCE`, each prefixed `c. ` when approximate, and
/// `undated` with no bound.
pub fn period_text(from: Option<i32>, to: Option<i32>, circa: bool) -> String {
    let text = match (from, to) {
        (None, None) => return "undated".to_string(),
        (Some(from), None) => format!("after {}", year_text(from)),
        (None, Some(to)) => format!("before {}", year_text(to)),
        (Some(from), Some(to)) if from == to => year_text(from),
        // Within one era the era is written once.
        (Some(from), Some(to)) if (from < 0) == (to < 0) => {
            format!("{}–{}", from.abs(), year_text(to))
        }
        (Some(from), Some(to)) => format!("{} – {}", year_text(from), year_text(to)),
    };
    if circa {
        format!("c. {}", text)
    } else {
        text
    }
}

/// Group the entries into clusters and put everything in order.
///
/// Clusters are ordered by sort year; then by `to`, an absent `to` last; then
/// by the tree position of their first entry. Entries inside a cluster, and the
/// undated ones, are in tree order.
pub fn build_timeline(mut entries: Vec<TimelineEntry>) -> Timeline {
    // Tree order first, so each cluster's entries arrive in it and a cluster's
    // first entry is its earliest in the tree.
    entries.sort_by(|a, b| a.tree_order().cmp(&b.tree_order()));

    let mut timeline = Timeline::default();
    let mut cluster_index: HashMap<(Option<i32>, Option<i32>, bool), usize> = HashMap::new();

    for entry in entries {
        if entry.from.is_none() && entry.to.is_none() {
            timeline.undated.push(entry);
            continue;
        }
        let key = (entry.from, entry.to, entry.circa);
        let index = *cluster_index.entry(key).or_insert_with(|| {
            timeline.clusters.push(TimelineCluster {
                from: entry.from,
                to: entry.to,
                circa: entry.circa,
                entries: Vec::new(),
            });
            timeline.clusters.len() - 1
        });
        timeline.clusters[index].entries.push(entry);
    }

    timeline.clusters.sort_by(|a, b| {
        let key = |c: &TimelineCluster| (c.sort_year(), c.to.is_none(), c.to);
        key(a)
            .cmp(&key(b))
            .then_with(|| a.entries[0].tree_order().cmp(&b.entries[0].tree_order()))
    });

    timeline
}

/// Every context note of `texts` whose node is in the tree, with that node's
/// titles and tree position. In no particular order.
///
/// A note whose key names no node is skipped: the tree build reports it.
pub fn timeline_entries_conn(
    db_conn: &mut SqliteConnection,
    texts: &NavigationTexts,
) -> QueryResult<Vec<TimelineEntry>> {
    let notes: HashMap<&str, _> = texts.dated_context_notes().collect();
    let uids: Vec<&str> = notes.keys().copied().collect();

    let nodes = navigation_nodes::table
        .filter(navigation_nodes::node_uid.eq_any(&uids))
        .select(NavigationNode::as_select())
        .load::<NavigationNode>(db_conn)?;

    if nodes.len() < notes.len() {
        let mut missing: Vec<&str> = uids
            .iter()
            .copied()
            .filter(|uid| !nodes.iter().any(|node| node.node_uid == *uid))
            .collect();
        missing.sort_unstable();
        warn(&format!(
            "navigation_timeline: {} context note(s) name no node and are left out: {}",
            missing.len(),
            missing.join(", ")
        ));
    }

    Ok(nodes
        .into_iter()
        .filter_map(|node| {
            let entry = notes.get(node.node_uid.as_str())?;
            Some(TimelineEntry {
                node_uid: node.node_uid,
                title_pali: node.title_pali,
                title_en: node.title_en,
                note: entry.note.clone(),
                from: entry.from,
                to: entry.to,
                circa: entry.circa,
                sort_path: node.sort_path,
            })
        })
        .collect())
}

/// The three ways a period is drawn, in plain words. `data-form` names the
/// form for the script, which draws a sample of it beside the text.
const LEGEND_ITEMS: [(&str, &str); 3] = [
    ("dated", "filled: dated range"),
    ("approximate", "hollow: approximate"),
    ("open", "arrow: only an earliest or latest date is known"),
];

fn render_legend(html: &mut String) {
    let items: Vec<String> = LEGEND_ITEMS
        .iter()
        .map(|(form, text)| {
            format!("<span class=\"timeline-legend-item\" data-form=\"{}\">{}</span>", form, escape(text))
        })
        .collect();
    html.push_str(&format!("<p class=\"timeline-legend\">{}</p>\n", items.join(" · ")));
}

fn escape(text: &str) -> std::borrow::Cow<'_, str> {
    html_escape::encode_double_quoted_attribute(text)
}

fn render_entry(html: &mut String, entry: &TimelineEntry) {
    html.push_str(&format!(
        "<article class=\"timeline-entry\" data-node-uid=\"{}\">\n",
        escape(&entry.node_uid)
    ));
    html.push_str(&format!(
        "<h3 class=\"timeline-title-pali\">{}</h3>\n",
        escape(entry.title())
    ));
    if let Some(subtitle) = entry.subtitle() {
        html.push_str(&format!(
            "<p class=\"timeline-title-en\">{}</p>\n",
            escape(subtitle)
        ));
    }
    html.push_str("<div class=\"timeline-note\">");
    for paragraph in entry.note.split("\n\n").filter(|p| !p.trim().is_empty()) {
        html.push_str(&format!("<p>{}</p>", escape(paragraph)));
    }
    html.push_str("</div>\n</article>\n");
}

fn render_cluster(html: &mut String, cluster: &TimelineCluster) {
    html.push_str("<section class=\"timeline-cluster\"");
    // An absent bound is an absent attribute: the script reads a missing
    // attribute as "no bound", and an empty one as the number 0.
    if let Some(from) = cluster.from {
        html.push_str(&format!(" data-from=\"{}\"", from));
    }
    if let Some(to) = cluster.to {
        html.push_str(&format!(" data-to=\"{}\"", to));
    }
    html.push_str(&format!(
        " data-circa=\"{}\" data-sort-year=\"{}\">\n",
        cluster.circa,
        cluster.sort_year()
    ));
    html.push_str(&format!(
        "<h2 class=\"timeline-period\">{}</h2>\n",
        escape(&period_text(cluster.from, cluster.to, cluster.circa))
    ));
    for entry in &cluster.entries {
        render_entry(html, entry);
    }
    html.push_str("</section>\n");
}

/// The page content: the controls, the clusters beside an empty `<svg>` for the
/// script to draw the axis in, and the undated entries.
///
/// Complete and readable as it is, with no script. All text is ordinary HTML,
/// which is what the find bar searches. With nothing to show, the controls are
/// followed by a one-line message.
pub fn render_timeline_html(timeline: &Timeline) -> String {
    let mut html = String::new();
    html.push_str("<div id=\"timeline\" class=\"timeline\">\n");
    html.push_str("<div class=\"timeline-controls\">\n");
    html.push_str(
        "<div class=\"timeline-toggles\">\n\
         <label><input type=\"checkbox\" id=\"timeline_scale_toggle\"> Scale spacing with time</label>\n\
         <label><input type=\"checkbox\" id=\"timeline_minimap_toggle\" checked> Minimap</label>\n\
         </div>\n",
    );
    render_legend(&mut html);
    html.push_str("</div>\n");

    if timeline.is_empty() {
        html.push_str("<p class=\"timeline-empty\">No timeline data</p>\n</div>\n");
        return html;
    }

    html.push_str("<div class=\"timeline-body\">\n");
    html.push_str("<svg class=\"timeline-axis\" aria-hidden=\"true\"></svg>\n");
    html.push_str("<div class=\"timeline-clusters\">\n");
    for cluster in &timeline.clusters {
        render_cluster(&mut html, cluster);
    }
    html.push_str("</div>\n</div>\n");

    if !timeline.undated.is_empty() {
        html.push_str("<section class=\"timeline-undated\">\n");
        html.push_str("<h2 class=\"timeline-period\">Undated</h2>\n");
        for entry in &timeline.undated {
            render_entry(&mut html, entry);
        }
        html.push_str("</section>\n");
    }

    html.push_str("</div>\n");
    html
}

/// The page content for the app's own tree and embedded texts.
///
/// A missing or unreadable tree gives the "No timeline data" content, never an
/// error. This parses the embedded texts if nothing has yet, so it is for a
/// worker thread: not the startup path and not the UI thread.
pub fn timeline_content_html() -> String {
    let entries = with_appdata("timeline_entries", Vec::new(), |db_conn| {
        timeline_entries_conn(db_conn, navigation_texts())
    });
    render_timeline_html(&build_timeline(entries))
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;

    use super::*;
    use crate::navigation_tree::{join_sort_path, NewNavigationNode};

    fn entry(node_uid: &str, from: Option<i32>, to: Option<i32>, circa: bool, sort_path: &str) -> TimelineEntry {
        TimelineEntry {
            node_uid: node_uid.to_string(),
            title_pali: Some(format!("{} pali", node_uid)),
            title_en: Some(format!("{} en", node_uid)),
            note: format!("Origin: {}.", node_uid),
            from,
            to,
            circa,
            sort_path: sort_path.to_string(),
        }
    }

    fn cluster_uids(timeline: &Timeline) -> Vec<Vec<&str>> {
        timeline
            .clusters
            .iter()
            .map(|c| c.entries.iter().map(|e| e.node_uid.as_str()).collect())
            .collect()
    }

    #[test]
    fn legend_names_each_form_for_the_script() {
        let html = render_timeline_html(&build_timeline(Vec::new()));
        for form in ["dated", "approximate", "open"] {
            assert!(html.contains(&format!("class=\"timeline-legend-item\" data-form=\"{}\"", form)));
        }
        assert!(html.contains(">filled: dated range</span> · <span"));
    }

    #[test]
    fn controls_have_both_toggles_and_the_minimap_starts_on() {
        let html = render_timeline_html(&build_timeline(Vec::new()));
        assert!(html.contains("<input type=\"checkbox\" id=\"timeline_scale_toggle\"> Scale spacing with time"));
        assert!(html.contains("<input type=\"checkbox\" id=\"timeline_minimap_toggle\" checked> Minimap"));
    }

    #[test]
    fn period_text_within_bce() {
        assert_eq!(period_text(Some(-500), Some(-200), false), "500–200 BCE");
    }

    #[test]
    fn period_text_across_the_eras() {
        assert_eq!(period_text(Some(-100), Some(200), false), "100 BCE – 200 CE");
    }

    #[test]
    fn period_text_within_ce() {
        assert_eq!(period_text(Some(1100), Some(1200), false), "1100–1200 CE");
    }

    #[test]
    fn period_text_from_only() {
        assert_eq!(period_text(Some(-300), None, false), "after 300 BCE");
    }

    #[test]
    fn period_text_to_only() {
        assert_eq!(period_text(None, Some(-100), false), "before 100 BCE");
    }

    #[test]
    fn period_text_circa_prefixes_every_dated_form() {
        assert_eq!(period_text(Some(-500), Some(-200), true), "c. 500–200 BCE");
        assert_eq!(period_text(Some(-100), Some(200), true), "c. 100 BCE – 200 CE");
        assert_eq!(period_text(Some(-300), None, true), "c. after 300 BCE");
        assert_eq!(period_text(None, Some(-100), true), "c. before 100 BCE");
        assert_eq!(period_text(Some(430), Some(430), true), "c. 430 CE");
    }

    #[test]
    fn period_text_undated_ignores_circa() {
        assert_eq!(period_text(None, None, false), "undated");
        assert_eq!(period_text(None, None, true), "undated");
    }

    #[test]
    fn period_text_single_year() {
        assert_eq!(period_text(Some(-250), Some(-250), false), "250 BCE");
    }

    #[test]
    fn entries_with_the_same_period_share_a_cluster() {
        let timeline = build_timeline(vec![
            entry("mn", Some(-500), Some(-200), false, "0001.0002"),
            entry("dn", Some(-500), Some(-200), false, "0001.0001"),
            entry("mil", Some(-100), Some(200), true, "0001.0009"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["dn", "mn"], vec!["mil"]]);
        assert!(timeline.undated.is_empty());
    }

    #[test]
    fn circa_is_part_of_the_period() {
        let timeline = build_timeline(vec![
            entry("a", Some(-500), Some(-200), true, "0002"),
            entry("b", Some(-500), Some(-200), false, "0001"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["b"], vec!["a"]]);
    }

    #[test]
    fn same_from_and_different_to_are_two_clusters_in_to_order() {
        let timeline = build_timeline(vec![
            entry("later", Some(-500), Some(-200), false, "0001"),
            entry("earlier", Some(-500), Some(-300), false, "0002"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["earlier"], vec!["later"]]);
    }

    #[test]
    fn open_ended_cluster_sorts_after_a_bounded_one_with_the_same_sort_year() {
        let timeline = build_timeline(vec![
            entry("open", Some(400), None, false, "0001"),
            entry("bounded", Some(400), Some(1900), false, "0002"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["bounded"], vec!["open"]]);
    }

    #[test]
    fn to_only_entry_sorts_by_its_to() {
        let timeline = build_timeline(vec![
            entry("late", Some(100), Some(200), false, "0001"),
            entry("before", None, Some(-100), true, "0002"),
            entry("early", Some(-500), Some(-200), false, "0003"),
        ]);
        assert_eq!(
            cluster_uids(&timeline),
            vec![vec!["early"], vec!["before"], vec!["late"]]
        );
        assert_eq!(timeline.clusters[1].sort_year(), -100);
    }

    #[test]
    fn entries_inside_a_cluster_follow_sort_path() {
        let timeline = build_timeline(vec![
            entry("c", Some(1), Some(2), false, "0002.0001"),
            entry("a", Some(1), Some(2), false, "0001.0003"),
            entry("b", Some(1), Some(2), false, "0001.0010"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["a", "b", "c"]]);
    }

    #[test]
    fn entry_with_no_bounds_is_undated() {
        let timeline = build_timeline(vec![
            entry("z", None, None, false, "0002"),
            entry("dn", Some(-500), Some(-200), false, "0003"),
            // `circa` alone dates nothing.
            entry("y", None, None, true, "0001"),
        ]);
        assert_eq!(cluster_uids(&timeline), vec![vec!["dn"]]);
        let undated: Vec<&str> = timeline.undated.iter().map(|e| e.node_uid.as_str()).collect();
        assert_eq!(undated, vec!["y", "z"]);
    }

    const UP_SQL: &[&str] = &[
        include_str!("../migrations/appdata/2026-09-19-000000_navigation_tree/up.sql"),
        include_str!("../migrations/appdata/2026-09-22-000000_navigation_node_redirect/up.sql"),
    ];

    fn tree_node(node_uid: &str, sort_index: i32, title_pali: &str, title_en: Option<&str>) -> NewNavigationNode {
        NewNavigationNode {
            node_uid: node_uid.to_string(),
            parent_uid: None,
            sort_index,
            node_type: "group".to_string(),
            tree_source: "sc".to_string(),
            title_pali: Some(title_pali.to_string()),
            title_en: title_en.map(|s| s.to_string()),
            sutta_uid: None,
            redirect_uid: None,
            cst_file: None,
            has_children: false,
            child_count: 0,
            is_available: true,
            search_text: String::new(),
            path: String::new(),
            sort_path: join_sort_path(&[], sort_index),
        }
    }

    fn tree_db(nodes: &[NewNavigationNode]) -> SqliteConnection {
        let mut db_conn = SqliteConnection::establish(":memory:").expect("in-memory db");
        for sql in UP_SQL {
            db_conn.batch_execute(sql).expect("migration applies");
        }
        diesel::insert_into(navigation_nodes::table)
            .values(nodes)
            .execute(&mut db_conn)
            .expect("seed inserts");
        db_conn
    }

    #[test]
    fn entries_join_the_notes_to_their_nodes() {
        let mut db_conn = tree_db(&[
            tree_node("mn", 1, "Majjhimanikāya", Some("Middle Discourses")),
            tree_node("dn", 0, "Dīghanikāya", Some("Long Discourses")),
            tree_node("an", 2, "Aṅguttaranikāya", None),
        ]);
        let texts = NavigationTexts::from_toml(
            "",
            "",
            r#"
["dn"]
from = -500
to = -200
note = "Origin: northern India."

["mn"]
to = -100
circa = true
note = "Origin: See Dīghanikāya."

["no-such-node"]
note = "Origin: nowhere."
"#,
        );

        let mut entries = timeline_entries_conn(&mut db_conn, &texts).unwrap();
        entries.sort_by(|a, b| a.sort_path.cmp(&b.sort_path));

        // `an` has no note and `no-such-node` has no node.
        assert_eq!(entries.len(), 2);

        let dn = &entries[0];
        assert_eq!(dn.node_uid, "dn");
        assert_eq!(dn.title_pali.as_deref(), Some("Dīghanikāya"));
        assert_eq!(dn.title_en.as_deref(), Some("Long Discourses"));
        assert_eq!(dn.note, "Origin: northern India.");
        assert_eq!((dn.from, dn.to, dn.circa), (Some(-500), Some(-200), false));

        let mn = &entries[1];
        assert_eq!(mn.node_uid, "mn");
        assert_eq!((mn.from, mn.to, mn.circa), (None, Some(-100), true));
        assert!(dn.sort_path < mn.sort_path);
    }

    #[test]
    fn entries_of_a_missing_table_are_an_error_not_a_panic() {
        let mut db_conn = SqliteConnection::establish(":memory:").expect("in-memory db");
        let texts = NavigationTexts::from_toml("", "", "[\"dn\"]\nnote = \"Origin.\"\n");
        assert!(timeline_entries_conn(&mut db_conn, &texts).is_err());
    }

    #[test]
    fn titles_are_the_ones_the_navigation_row_shows() {
        let mut e = entry("dn", None, None, false, "0001");
        assert_eq!(e.title(), "dn pali");
        assert_eq!(e.subtitle(), Some("dn en"));

        // No Pāli title: the English one moves up and is not repeated.
        e.title_pali = None;
        assert_eq!(e.title(), "dn en");
        assert_eq!(e.subtitle(), None);

        e.title_en = Some(String::new());
        assert_eq!(e.title(), "dn");
        assert_eq!(e.subtitle(), None);
    }

    #[test]
    fn html_escapes_titles_and_notes() {
        let mut e = entry("dn", Some(-500), Some(-200), false, "0001");
        e.title_pali = Some("A <b> & B".to_string());
        e.title_en = Some("C & <i>D</i>".to_string());
        e.note = "x < y & \"z\"".to_string();
        let html = render_timeline_html(&build_timeline(vec![e]));

        assert!(html.contains("<h3 class=\"timeline-title-pali\">A &lt;b&gt; &amp; B</h3>"));
        assert!(html.contains("<p class=\"timeline-title-en\">C &amp; &lt;i&gt;D&lt;/i&gt;</p>"));
        assert!(html.contains("<p>x &lt; y &amp; &quot;z&quot;</p>"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn html_carries_the_dates_as_data_attributes() {
        let html = render_timeline_html(&build_timeline(vec![entry(
            "dn",
            Some(-500),
            Some(-200),
            true,
            "0001",
        )]));
        assert!(html.contains(
            "<section class=\"timeline-cluster\" data-from=\"-500\" data-to=\"-200\" data-circa=\"true\" data-sort-year=\"-500\">"
        ));
        assert!(html.contains("<h2 class=\"timeline-period\">c. 500–200 BCE</h2>"));
        assert!(html.contains("<article class=\"timeline-entry\" data-node-uid=\"dn\">"));
    }

    #[test]
    fn html_has_no_attribute_for_an_absent_bound() {
        let html = render_timeline_html(&build_timeline(vec![
            entry("a", Some(-300), None, true, "0001"),
            entry("b", None, Some(-100), false, "0002"),
        ]));
        assert!(html.contains(
            "<section class=\"timeline-cluster\" data-from=\"-300\" data-circa=\"true\" data-sort-year=\"-300\">"
        ));
        assert!(html.contains(
            "<section class=\"timeline-cluster\" data-to=\"-100\" data-circa=\"false\" data-sort-year=\"-100\">"
        ));
        assert!(!html.contains("data-to=\"\""));
        assert!(!html.contains("data-from=\"\""));
    }

    #[test]
    fn html_note_paragraphs_become_p_elements() {
        let mut e = entry("dn", Some(1), Some(2), false, "0001");
        e.note = "First.\n\nSecond.".to_string();
        let html = render_timeline_html(&build_timeline(vec![e]));
        assert!(html.contains("<div class=\"timeline-note\"><p>First.</p><p>Second.</p></div>"));
    }

    #[test]
    fn html_undated_section_only_when_there_are_undated_entries() {
        let dated = entry("dn", Some(1), Some(2), false, "0001");
        let html = render_timeline_html(&build_timeline(vec![dated.clone()]));
        assert!(!html.contains("timeline-undated"));

        let html = render_timeline_html(&build_timeline(vec![
            dated,
            entry("x", None, None, false, "0002"),
        ]));
        assert!(html.contains(
            "<section class=\"timeline-undated\">\n<h2 class=\"timeline-period\">Undated</h2>\n<article class=\"timeline-entry\" data-node-uid=\"x\">"
        ));
    }

    #[test]
    fn html_entry_without_english_title_has_no_en_element() {
        let mut e = entry("dn", Some(1), Some(2), false, "0001");
        e.title_en = None;
        let html = render_timeline_html(&build_timeline(vec![e]));
        assert!(!html.contains("timeline-title-en"));
    }

    #[test]
    fn html_of_an_empty_timeline_is_the_controls_and_a_message() {
        let html = render_timeline_html(&Timeline::default());
        assert!(html.contains("id=\"timeline_scale_toggle\""));
        assert!(html.contains("<p class=\"timeline-empty\">No timeline data</p>"));
        assert!(!html.contains("timeline-body"));
        assert_eq!(html.matches("<div").count(), html.matches("</div>").count());
    }

    #[test]
    fn html_tags_are_balanced() {
        let html = render_timeline_html(&build_timeline(vec![
            entry("dn", Some(-500), Some(-200), false, "0001"),
            entry("x", None, None, false, "0002"),
        ]));
        for tag in ["div", "section", "article"] {
            assert_eq!(
                html.matches(&format!("<{}", tag)).count(),
                html.matches(&format!("</{}>", tag)).count(),
                "{}",
                tag
            );
        }
    }

    #[test]
    fn shipped_notes_all_have_a_period_text() {
        let texts = navigation_texts();
        for (uid, entry) in texts.dated_context_notes() {
            let text = period_text(entry.from, entry.to, entry.circa);
            assert!(!text.is_empty(), "{}", uid);
            // A year 0 would print as "0 CE".
            assert!(!text.contains(" 0 CE") && !text.starts_with("0 CE"), "{}: {}", uid, text);
        }
    }
}
