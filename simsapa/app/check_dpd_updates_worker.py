from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger, DPD_RELEASES_BASE_URL
from simsapa.app.db_session import get_dpd_db_version
from simsapa.app.helpers import is_valid_date
from simsapa.layouts.gui_helpers import get_version_tags_from_github_feed

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

        feed_url = f"{DPD_RELEASES_BASE_URL}/releases.atom"

        # Test if connection to is working.
        try:
            import requests
            requests.head(feed_url, timeout=5)
        except Exception as e:
            logger.error("No Connection: Update info unavailable: %s" % e)
            return None

        try:
            version_tags = get_version_tags_from_github_feed(feed_url)

            # FIXME show user the error messages

            if len(version_tags) == 0:
                logger.error(f"Cannot get the latest dpd_db version info from url: {feed_url}.")
                self.signals.no_updates.emit()
                return

            dpd_release_version_tag = version_tags[0]

            # Currently the DPD version is the release date, e.g 2023-11-27
            # Check that version tag is in this format.
            if not is_valid_date(dpd_release_version_tag):
                logger.error(f"Version tag is not iso date: {dpd_release_version_tag}")
                return None

            remote_version = int(dpd_release_version_tag.replace("-", ""))

            res = get_dpd_db_version()
            if res is None:
                logger.error("Cannot determine local dpd_db version.")
                self.signals.no_updates.emit()
                return

            local_version = int(res.replace("-", ""))

            if remote_version > local_version:
                # update_info = "DPD version available"
                # releases_info = "DPD version available"
                # FIXME value = {"update_info": update_info, "releases_info": releases_info}
                value = {"dpd_release_version_tag": dpd_release_version_tag}
                self.signals.have_dpd_update.emit(value)
                return

            self.signals.no_updates.emit()

        except Exception as e:
            logger.error(e)
