#ifndef SYSTEM_PALETTE_H_
#define SYSTEM_PALETTE_H_

#include <QString>

// Get the system palette colors as JSON string
QString get_system_palette_json();

// Set the Link / LinkVisited roles on the *application* palette.
//
// Rich text (`Text { textFormat: Text.RichText }`) renders an <a href> through
// QTextDocument, and Qt's HTML parser hard-codes the anchor colour to
// `palette(link)` (qtexthtmlparser.cpp:2062-2065). That declaration is resolved
// by `QCss::ValueExtractor`, which is constructed without a palette argument
// (qtexthtmlparser.cpp:1182) and so falls back to a default-constructed
// QPalette — i.e. QGuiApplication's, never the QML window's palette that
// ThemeHelper assigns. The resulting explicit foreground also *beats*
// `Text.linkColor`, which QQuickTextNodeEngine only applies when the char
// format has no foreground of its own
// (qquicktextnodeengine.cpp:1098-1101).
//
// So this is the only knob that colours rich-text links, and it has to be set
// per theme. Only these two roles are touched, to keep the blast radius of
// writing to the application palette as small as possible.
void set_app_palette_link_colors(const QString &link, const QString &link_visited);

#endif // SYSTEM_PALETTE_H_
