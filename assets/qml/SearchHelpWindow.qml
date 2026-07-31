import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: root

    title: "Search Queries"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile ? 14 : 12
    required property int extra_top_margin
    property bool is_dark: theme_helper.is_dark

    // The Search Queries documentation page. Shown as a link in the body and
    // opened by the "Open Link" button at the bottom of the window.
    readonly property string docs_url: "https://simsapa.github.io/features/search-queries/"

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    function show_help() {
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            spacing: 0
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.leftMargin: 10
            anchors.rightMargin: 10

            // Scrollable content area
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    Text {
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        color: palette.text
                        textFormat: Text.RichText
                        wrapMode: Text.Wrap
                        onLinkActivated: (link) => Qt.openUrlExternally(link)
                        text: `
<h2>Search Help</h2>

<p>Use the search bar to look in the <b>Suttas</b> (S), <b>Dictionary</b> (D) or
<b>Library</b> (L) areas. Choose the area with the S / D / L buttons, then pick a
search mode from the dropdown.</p>

<h3>Search modes</h3>
<ul>
    <li><b>Fulltext Match</b> — word-based full-text search over the whole text.
    Pāli words are stemmed, so a query matches its inflected forms. Best for
    finding a word anywhere in the texts.</li>
    <li><b>Contains Match</b> — literal substring match. Finds the exact letters
    you typed, wherever they occur.</li>
    <li><b>Title Match</b> — matches only the titles of suttas or books.</li>
    <li><b>Combined</b> (Dictionary) — runs a DPD lookup together with the word
    deconstructor, the most helpful default for dictionary searches.</li>
    <li><b>DPD Lookup</b> — looks up headwords and roots in the Digital Pāḷi
    Dictionary.</li>
    <li><b>Headword Match</b> — matches dictionary headwords.</li>
</ul>
<h3>Fulltext Search Examples</h3>

<p>The fulltext search uses <a href="https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html">tantivy’s query syntax</a>. The ‘must’ (+) and ‘negative’ (-) terms are particulary useful for filtering results.</p>
<p>Words don’t have to be exactly near each other, e.g. <b>so ce evam vadeyya</b> will also find <b>so ce</b> bhikkhu <b>evaṁ vadeyya</b></p>
<p>Prefixing a term with + and - can control “Must” or “Must not” include.</p>
<p><b>santam padam abhisamecca</b> – each term may be included, but ok if not all are found.</p>
<p><b>santam padam +abhisamecca</b> – ‘abhisamecca’ must be included, even if the others may be absent.</p>
<p><b>santam padam -abhisamecca</b> – ‘abhisamecca’ must not be included.</p>
<p>Fulltext matches Pāli declensions but doesn’t do partial word matches, so <b>upasan</b> doesn’t find anything (not a valid declension stem) until you type <b>upasankama</b>.</p>
<p>The <em>Contains Match</em> is for exact partial matches.</p>

<h3>Typing a query</h3>
<ul>
    <li>Search-as-you-type runs a plain text query once it is <b>4 or more
    characters</b> long; a recognised reference such as <code>mn8</code> runs
    from <b>3</b>. This way, short text queries which would match too many results
    are held back while you type.</li>
    <li>Press the <b>search button</b> (or the Enter key) to run a query without
    waiting: a <b>3-character</b> query runs straight away, while a <b>1- or
    2-character</b> query first asks you to confirm, because it can return
    many results.</li>
    <li>In the <b>Dictionary</b> area, a one- or two-letter word such as
    <code>i</code> or <code>ko</code> can be looked up as a dictionary id by
    using the <code>/dpd</code> form (<code>i/dpd</code>, <code>ko/dpd</code>).
    Confirming the short-query prompt does this for you.</li>
</ul>

<h3>References open directly</h3>
<ul>
    <li>A sutta reference such as <code>SN 56.11</code>, <code>Dhp 183</code> or a
    book uid such as <code>bmc.0</code> opens that text.</li>
    <li>A dictionary reference such as <code>dhamma 1.01</code>,
    <code>34626/dpd</code> or <code>dhamma-1-01/dpd</code> opens that entry.</li>
</ul>

<h3>Language filter</h3>
<p>The language dropdown limits results to one language. <b>Language</b> (the
first option) means no filter — results from all languages are shown.</p>

<h3>Query length summary</h3>
<table border="1" cellspacing="0" cellpadding="6" width="100%">
    <tr>
        <th align="left">Length</th>
        <th align="left">Search-as-you-type</th>
        <th align="left">Search button</th>
    </tr>
    <tr>
        <td>0</td>
        <td>nothing</td>
        <td>nothing</td>
    </tr>
    <tr>
        <td>1</td>
        <td>held back</td>
        <td>confirm dialog</td>
    </tr>
    <tr>
        <td>2</td>
        <td>held back</td>
        <td>confirm dialog</td>
    </tr>
    <tr>
        <td>3</td>
        <td>reference (<code>mn8</code>) runs; text (<code>eva</code>) held back</td>
        <td>runs, no confirm</td>
    </tr>
    <tr>
        <td>4</td>
        <td>text (<code>dasa</code>) runs</td>
        <td>runs, no confirm</td>
    </tr>
</table>

<p>Full documentation:
<a href="${root.docs_url}">${root.docs_url}</a></p>
`

                        MouseArea {
                            anchors.fill: parent
                            acceptedButtons: Qt.NoButton // we don't want to eat clicks on the Text
                            cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                        }
                    }

                    Item {
                        Layout.fillHeight: true
                    }
                }
            }

            // Fixed button area at the bottom
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 20
                Layout.bottomMargin: 20

                Item { Layout.fillWidth: true }

                Button {
                    text: "Open Link"
                    font.pointSize: root.pointSize
                    onClicked: {
                        Qt.openUrlExternally(root.docs_url);
                        root.close();
                    }
                }

                Button {
                    text: "Close"
                    font.pointSize: root.pointSize
                    onClicked: root.close()
                }

                Item { Layout.fillWidth: true }
            }
        }
    }
}
