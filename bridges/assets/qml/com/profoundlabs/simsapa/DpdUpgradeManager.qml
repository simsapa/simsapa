import QtQuick

QtObject {
    signal upgradeProgress(progress_json: string);
    signal upgradeCompleted(report_json: string);
    signal upgradeFailed(error_json: string);

    function pending_version(): string {
        return "v0.4.20261026";
    }

    function start(): bool {
        console.log("start()");
        return true;
    }

    function cancel() {
        console.log("cancel()");
    }
}
