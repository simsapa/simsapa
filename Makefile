all:
	@echo "No default target."

icons:
	pyrcc5 -o simsapa/assets/icons_rc.py simsapa/assets/icons.qrc

ui:
	pyuic5 --import-from=simsapa.assets -o simsapa/assets/ui/sutta_search_window_ui.py simsapa/assets/ui/sutta_search_window.ui && \
	pyuic5 --import-from=simsapa.assets -o simsapa/assets/ui/dictionary_search_window_ui.py simsapa/assets/ui/dictionary_search_window.ui && \
	pyuic5 --import-from=simsapa.assets -o simsapa/assets/ui/document_reader_window_ui.py simsapa/assets/ui/document_reader_window.ui && \
	pyuic5 --import-from=simsapa.assets -o simsapa/assets/ui/library_browser_window_ui.py simsapa/assets/ui/library_browser_window.ui && \
	pyuic5 --import-from=simsapa.assets -o simsapa/assets/ui/notes_browser_window_ui.py simsapa/assets/ui/notes_browser_window.ui

