from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger, DPD_RELEASES_REPO_URL
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
            feed_url = f"{DPD_RELEASES_REPO_URL}/releases.atom"
            requests.head(feed_url, timeout=5)
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

        message = f"<div><p><b>{entry['title']}</b></p>{entry['description']}</div>"

        update_info = UpdateInfo(
            version = entry['version_tag'],
            message = message,
            visit_url = f"https://github.com/{entry['github_repo']}/releases/tag/{entry['version_tag']}"
        )

        value = {"update_info": update_info, "releases_info": ""}
        self.signals.have_dpd_update.emit(value)
