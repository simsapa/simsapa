#ifndef UTILS_H
#define UTILS_H

#include <QString>

extern "C++" {
    QString get_internal_storage_path();
    QString get_app_assets_path();
    QString get_app_data_storage_paths_json();
    int get_status_bar_height();
    QString copy_content_uri_to_temp_file(const QString& content_uri);
    QString get_qt_platform_name();
    QString get_qt_version();
    QString get_android_package_name();
    QString get_installer_package_name();
}

QString copy_apk_assets_to_internal_storage(QString apk_asset_path = QString());
QStringList list_qrc_assets();
QString copy_qrc_app_assets_to_internal_storage();

#endif
