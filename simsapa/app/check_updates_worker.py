from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from simsapa import logger, SIMSAPA_RELEASES_BASE_URL

from simsapa.layouts.gui_helpers import EntryType, get_releases_info, has_update, is_local_db_obsolete

class UpdatesWorkerSignals(QObject):
    have_app_update = pyqtSignal(dict)
    have_db_update = pyqtSignal(dict)
    local_db_obsolete = pyqtSignal(dict)
    no_updates = pyqtSignal()

class CheckUpdatesWorker(QRunnable):
    signals: UpdatesWorkerSignals

    def __init__(self, save_stats = True, screen_size = ''):
        super().__init__()
        self.signals = UpdatesWorkerSignals()
        self._screen_size = screen_size
        self._save_stats = save_stats

    @pyqtSlot()
    def run(self):
        logger.profile("CheckUpdatesWorker::run()")
        # Test if connection to is working.
        try:
            import requests
            requests.head(SIMSAPA_RELEASES_BASE_URL, timeout=5)
        except Exception as e:
            logger.error("No Connection: Update info unavailable: %s" % e)
            return None

        try:
            info = get_releases_info(save_stats=self._save_stats, screen_size=self._screen_size)

            update_info = is_local_db_obsolete()
            if update_info is not None:
                value = {"update_info": update_info, "releases_info": info}
                self.signals.local_db_obsolete.emit(value)
                return

            update_info = has_update(info, EntryType.Application)
            if update_info is not None:
                value = {"update_info": update_info, "releases_info": info}
                self.signals.have_app_update.emit(value)
                return

            update_info = has_update(info, EntryType.Assets)
            if update_info is not None:
                value = {"update_info": update_info, "releases_info": info}
                self.signals.have_db_update.emit(value)
                return

            self.signals.no_updates.emit()

        except Exception as e:
            logger.error(e)
