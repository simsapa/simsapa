import QtQuick

Item {
    function copyWithMimeType(text: string, mimeType: string) {
        console.log("copyWithMimeType():", text.slice(0, 50), mimeType);
    }
}
