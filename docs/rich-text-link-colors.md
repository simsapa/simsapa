# Rich-text `<a href>` links are coloured by the application palette

Setting `Text.linkColor`, or the window's `palette.link`, does **nothing** for a
link inside `Text { textFormat: Text.RichText }`. Qt's HTML parser injects
`color: palette(link)` for every `<a href>` (`qtexthtmlparser.cpp:2062-2065`),
resolving it against a default-constructed `QPalette` — `QGuiApplication`'s, not
the window's (`qtexthtmlparser.cpp:1182`) — and the resulting explicit foreground
then **overrides** `linkColor`, which `QQuickTextNodeEngine` applies only when
the char format has none (`qquicktextnodeengine.cpp:1098-1101`).

The colour is therefore pushed into the application palette by
`set_app_palette_link_colors()` (`cpp/system_palette.cpp`). That is the **only**
knob; do not add per-item `linkColor` bindings expecting them to work. If a link
renders in the wrong colour, the theme JSONs
(`backend/src/theme_colors_{light,dark}.json`) are what to edit.

**It is applied in `gui.cpp` right after `QApplication` and before the QML engine
loads**, via `theme_link_colors_c()` (`backend/src/lib.rs`, a standalone settings
read sharing `render_loop_basic_c()`'s cache). That ordering is load-bearing: the
anchor colour is baked into the char format when the HTML is **parsed**, so any
window whose QML is parsed during the engine load — `SearchHelpWindow` and
`DhammaTextSourcesDialog` are inline children of `SuttaSearchWindow` — keeps the
platform default if the palette is only fixed afterwards from
`ThemeHelper.apply()`. (That call is kept too, for windows created after a
runtime theme change; already-parsed rich text needs a restart to recolour.)

