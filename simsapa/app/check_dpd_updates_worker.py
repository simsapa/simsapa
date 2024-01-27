from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

import markdown

from simsapa import DPD_RELEASES_API_URL, logger
from simsapa.layouts.gui_helpers import UpdateInfo, get_dpd_releases

class UpdatesWorkerSignals(QObject):
    have_dpd_update = pyqtSignal(dict)
    no_updates = pyqtSignal()

class CheckDpdUpdatesWorker(QRunnable):
    signals: UpdatesWorkerSignals

    def __init__(self, save_stats = True, screen_size = ''):
        super().__init__()
        self.signals = UpdatesWorkerSignals()
        self._screen_size = screen_size
        self._save_stats = save_stats

    @pyqtSlot()
    def run(self):
        logger.profile("CheckDpdUpdatesWorker::run()")

        # Test if connection to is working.
        try:
            import requests
            requests.head(DPD_RELEASES_API_URL, timeout=5)
        except Exception as e:
            logger.error("No Connection: Update info unavailable: %s" % e)
            return None

        try:
            releases = get_dpd_releases()
        except Exception as e:
            logger.error(e)
            self.signals.no_updates.emit()
            return

        # No greater or compatible versions are available.
        if len(releases) == 0:
            self.signals.no_updates.emit()
            return

        entry = releases[0]

        # DPD release description from the API JSON is in markdown.
        description_html = markdown.markdown(entry['description'].replace("\r\n", "<br/>\n"))

        message = f"<div><p><b>{entry['title']}</b></p>{description_html}</div>"

        update_info = UpdateInfo(
            version = entry['version_tag'],
            message = message,
            visit_url = f"https://github.com/{entry['github_repo']}/releases/tag/{entry['version_tag']}"
        )

        value = {"update_info": update_info, "releases_info": ""}
        self.signals.have_dpd_update.emit(value)
