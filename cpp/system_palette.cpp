#include "system_palette.h"
#include <QApplication>
#include <QPalette>
#include <QJsonObject>
#include <QJsonDocument>
#include <QGuiApplication>

void set_app_palette_link_colors(const QString &link, const QString &link_visited) {
    QColor link_color(link);
    QColor link_visited_color(link_visited);
    if (!link_color.isValid() && !link_visited_color.isValid()) {
        return;
    }

    QPalette palette = QGuiApplication::palette();

    // Set every colour group: QTextDocument's anchor lookup does not use the
    // group of the window that happens to be focused.
    for (auto group : {QPalette::Active, QPalette::Inactive, QPalette::Disabled}) {
        if (link_color.isValid()) {
            palette.setColor(group, QPalette::Link, link_color);
        }
        if (link_visited_color.isValid()) {
            palette.setColor(group, QPalette::LinkVisited, link_visited_color);
        }
    }

    QGuiApplication::setPalette(palette);
}

QString get_system_palette_json() {
    // Get the application's palette
    QPalette palette = QGuiApplication::palette();

    // Helper lambda to convert QColor to hex string
    auto to_hex = [](const QColor& color) -> QString {
        return QString("#%1").arg(color.rgb() & 0xFFFFFF, 6, 16, QChar('0')).toUpper();
    };

    // Helper lambda to extract colors for a specific color group
    auto extractColorGroup = [&](QPalette::ColorGroup g) -> QJsonObject {
        QJsonObject d;

        d["window"] = to_hex(palette.color(g, QPalette::Window));
        d["windowText"] = to_hex(palette.color(g, QPalette::WindowText));
        d["base"] = to_hex(palette.color(g, QPalette::Base));
        d["alternateBase"] = to_hex(palette.color(g, QPalette::AlternateBase));
        d["accent"] = to_hex(palette.color(g, QPalette::Accent));
        d["text"] = to_hex(palette.color(g, QPalette::Text));

        d["button"] = to_hex(palette.color(g, QPalette::Button));
        d["buttonText"] = to_hex(palette.color(g, QPalette::ButtonText));

        d["brightText"] = to_hex(palette.color(g, QPalette::BrightText));
        d["placeholderText"] = to_hex(palette.color(g, QPalette::PlaceholderText));

        d["highlight"] = to_hex(palette.color(g, QPalette::Highlight));
        d["highlightedText"] = to_hex(palette.color(g, QPalette::HighlightedText));
        d["toolTipBase"] = to_hex(palette.color(g, QPalette::ToolTipBase));
        d["toolTipText"] = to_hex(palette.color(g, QPalette::ToolTipText));

        d["light"] = to_hex(palette.color(g, QPalette::Light));
        d["midlight"] = to_hex(palette.color(g, QPalette::Midlight));
        d["dark"] = to_hex(palette.color(g, QPalette::Dark));
        d["mid"] = to_hex(palette.color(g, QPalette::Mid));
        d["shadow"] = to_hex(palette.color(g, QPalette::Shadow));
        d["link"] = to_hex(palette.color(g, QPalette::Link));
        d["linkVisited"] = to_hex(palette.color(g, QPalette::LinkVisited));

        return d;
    };

    // Create main JSON object with separate color groups
    QJsonObject paletteJson;
    paletteJson["active"] = extractColorGroup(QPalette::Active);
    paletteJson["inactive"] = extractColorGroup(QPalette::Inactive);
    paletteJson["disabled"] = extractColorGroup(QPalette::Disabled);

    // Convert to JSON document and return as string
    QJsonDocument doc(paletteJson);
    return doc.toJson(QJsonDocument::Compact);
}
