cmake_minimum_required(VERSION 3.24)

project(simsapadhammareader LANGUAGES CXX)

# Qt version per platform. Set these to match the installed Qt for each target.
# Used below to build CMAKE_PREFIX_PATH / qmake paths, so the version only needs
# to be changed in one place per platform.
set(QT_LINUX   "6.9.3")
set(QT_MACOS   "6.9.3")
set(QT_WINDOWS "6.9.3")
set(QT_ANDROID "6.9.3")
set(QT_IOS     "6.9.3")

# Rust always links against non-debug Windows runtime on *-msvc targets
# Note it is best to set this on the command line to ensure all targets are consistent
# https://github.com/corrosion-rs/corrosion/blob/master/doc/src/common_issues.md#linking-debug-cc-libraries-into-rust-fails-on-windows-msvc-targets
# https://github.com/rust-lang/rust/issues/39016
if (CMAKE_CXX_COMPILER_ID STREQUAL "MSVC")
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
endif()

# Detect and set CMAKE_PREFIX_PATH for Qt6 on different platforms
if (WIN32 AND NOT CMAKE_PREFIX_PATH)
    # Try to find Qt6 installation on Windows
    if(EXISTS "C:/Qt/${QT_WINDOWS}/msvc2022_64")
        set(CMAKE_PREFIX_PATH "C:/Qt/${QT_WINDOWS}/msvc2022_64")
    elseif(EXISTS "C:/Qt/${QT_WINDOWS}/msvc2019_64")
        set(CMAKE_PREFIX_PATH "C:/Qt/${QT_WINDOWS}/msvc2019_64")
    elseif(EXISTS "$ENV{USERPROFILE}/Qt/${QT_WINDOWS}/msvc2022_64")
        set(CMAKE_PREFIX_PATH "$ENV{USERPROFILE}/Qt/${QT_WINDOWS}/msvc2022_64")
    else()
        message(WARNING "Qt6 installation not found in standard Windows locations. Please set CMAKE_PREFIX_PATH manually.")
    endif()
    if(CMAKE_PREFIX_PATH)
        message(STATUS "Using CMAKE_PREFIX_PATH: ${CMAKE_PREFIX_PATH}")
    endif()
elseif (APPLE AND NOT IOS)
    # Find the macOS SDK path using xcrun (more reliable than hardcoding)
    execute_process(
        COMMAND xcrun --sdk macosx --show-sdk-path
        OUTPUT_VARIABLE MACOS_SDK_PATH
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )

    # Only set CMAKE_OSX_SYSROOT if we found a valid SDK path
    if(MACOS_SDK_PATH AND EXISTS "${MACOS_SDK_PATH}")
        set(CMAKE_OSX_SYSROOT "${MACOS_SDK_PATH}" CACHE PATH "macOS SDK path" FORCE)
        message(STATUS "Using macOS SDK: ${CMAKE_OSX_SYSROOT}")
    else()
        message(WARNING "macOS SDK not found via xcrun, using system default")
        # Unset to use system default
        unset(CMAKE_OSX_SYSROOT CACHE)
    endif()

    # Set macOS deployment target to match the SDK being used
    # This prevents version mismatch warnings during linking
    set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING "Minimum macOS version" FORCE)
    message(STATUS "macOS deployment target: ${CMAKE_OSX_DEPLOYMENT_TARGET}")

    # Try to find Qt6 installation if CMAKE_PREFIX_PATH is not already set
    if(NOT CMAKE_PREFIX_PATH)
        if(EXISTS "$ENV{HOME}/Qt/${QT_MACOS}/macos")
            set(CMAKE_PREFIX_PATH "$ENV{HOME}/Qt/${QT_MACOS}/macos")
        elseif(EXISTS "/opt/Qt/${QT_MACOS}/macos")
            set(CMAKE_PREFIX_PATH "/opt/Qt/${QT_MACOS}/macos")
        elseif(EXISTS "/usr/local/Qt/${QT_MACOS}/macos")
            set(CMAKE_PREFIX_PATH "/usr/local/Qt/${QT_MACOS}/macos")
        else()
            message(WARNING "Qt6 installation not found in standard locations. Please set CMAKE_PREFIX_PATH manually.")
        endif()
    endif()
    message(STATUS "Using CMAKE_PREFIX_PATH: ${CMAKE_PREFIX_PATH}")
endif()

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(CMAKE_AUTORCC_OPTIONS --format-version 1)
list(APPEND CMAKE_AUTORCC_OPTIONS --compress-algo zlib)

set(CXXQT_QTCOMPONENTS Core Gui Qml QuickControls2 QuickTest Test)

list(APPEND app_components Widgets Quick)

list(APPEND qt_modules
        Qt::Core
        Qt::Gui
        Qt::Qml
        Qt::Network
        Qt::Widgets
        Qt::Quick
        Qt::QuickControls2)

if (ANDROID)
    # Detect Android ABI and set appropriate Qt paths
    # ANDROID_ABI is set by the Android toolchain (e.g., armeabi-v7a, arm64-v8a, x86_64)
    #
    # MULTI-ABI NOTE: for a multi-ABI build (see build-android.sh /
    # `make android-aab`), Qt configures one ExternalProject per additional ABI,
    # re-running CMake over this same source tree with that ABI's toolchain
    # file. So ANDROID_ABI is correct in each sub-build and the branches below
    # resolve per-ABI. Qt does NOT forward the parent's CMAKE_PREFIX_PATH to the
    # sub-builds (only ANDROID_SDK_ROOT / ANDROID_NDK_ROOT and the variables
    # named in QT_ANDROID_MULTI_ABI_FORWARD_VARS), which is exactly what makes
    # the `if(NOT CMAKE_PREFIX_PATH)` guard below safe.
    # See docs/android-multi-abi-and-chromeos.md.
    if(ANDROID_ABI STREQUAL "armeabi-v7a")
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_armv7")
    elseif(ANDROID_ABI STREQUAL "arm64-v8a")
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_arm64_v8a")
    elseif(ANDROID_ABI STREQUAL "x86_64")
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_x86_64")
    elseif(ANDROID_ABI STREQUAL "x86")
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_x86")
    else()
        # Default to arm64-v8a if ABI not detected
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_arm64_v8a")
        message(WARNING "ANDROID_ABI not detected, defaulting to android_arm64_v8a")
    endif()

    if(NOT CMAKE_PREFIX_PATH)
        set(CMAKE_PREFIX_PATH "${ANDROID_QT_DIR}")
    endif()
    message(STATUS "Android ABI: ${ANDROID_ABI}")
    message(STATUS "Using Android Qt: ${ANDROID_QT_DIR}")

    set(qmake_path "${ANDROID_QT_DIR}/bin/qmake")
    list(APPEND app_components WebView)
    list(APPEND qt_modules Qt::WebView)
    add_compile_definitions(Q_OS_ANDROID)

    # Android package version. androiddeployqt substitutes these into
    # android/AndroidManifest.xml's %%INSERT_VERSION_CODE%% / %%INSERT_VERSION_NAME%%
    # placeholders. Google Play requires a strictly increasing versionCode on
    # every upload of a given package, so bump it for each release:
    #   make android-aab ANDROID_VERSION_CODE=3 ANDROID_VERSION_NAME=1.0.0-alpha.3
    # Without these properties Qt defaults to versionCode 1 / versionName "1.0".
    set(ANDROID_VERSION_CODE "1" CACHE STRING "Android manifest versionCode")
    set(ANDROID_VERSION_NAME "1.0.0-alpha.2" CACHE STRING "Android manifest versionName")
elseif (IOS)
    set(Rust_CARGO_TARGET aarch64-apple-ios)
    set(qmake_path "$ENV{HOME}/Qt/${QT_IOS}/ios/bin/qmake6")
    list(APPEND app_components WebView)
    list(APPEND qt_modules Qt::WebView)
elseif (APPLE)
    # Explicitly set Rust target for macOS to prevent iOS cross-compilation
    if(CMAKE_HOST_SYSTEM_PROCESSOR MATCHES "arm64|aarch64")
        set(Rust_CARGO_TARGET aarch64-apple-darwin)
    else()
        set(Rust_CARGO_TARGET x86_64-apple-darwin)
    endif()

    set(qmake_path "$ENV{HOME}/Qt/${QT_MACOS}/macos/bin/qmake6")
    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
elseif (WIN32)
    # Explicitly set Rust target for Windows MSVC
    set(Rust_CARGO_TARGET x86_64-pc-windows-msvc)

    # Try to find qmake in Qt installation
    if(EXISTS "C:/Qt/${QT_WINDOWS}/msvc2022_64/bin/qmake6.exe")
        set(qmake_path "C:/Qt/${QT_WINDOWS}/msvc2022_64/bin/qmake6.exe")
    elseif(EXISTS "C:/Qt/${QT_WINDOWS}/msvc2019_64/bin/qmake6.exe")
        set(qmake_path "C:/Qt/${QT_WINDOWS}/msvc2019_64/bin/qmake6.exe")
    elseif(EXISTS "$ENV{USERPROFILE}/Qt/${QT_WINDOWS}/msvc2022_64/bin/qmake6.exe")
        set(qmake_path "$ENV{USERPROFILE}/Qt/${QT_WINDOWS}/msvc2022_64/bin/qmake6.exe")
    else()
        # Fallback to using qmake from PATH
        set(qmake_path "qmake6")
    endif()

    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
else()
    set(qmake_path "$ENV{HOME}/Qt/${QT_LINUX}/gcc_64/bin/qmake6")
    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
endif()

find_package(Qt6 COMPONENTS ${CXXQT_QTCOMPONENTS} ${app_components})

find_package(CxxQt QUIET)
if(NOT CxxQt_FOUND)
    include(FetchContent)
    FetchContent_Declare(
        CxxQt
        GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
        GIT_TAG 0.7
    )

    FetchContent_MakeAvailable(CxxQt)
endif()

# CXX-Qt (using Corrosion) creates a CMake target with the same name as the crate.
cxx_qt_import_crate(
    MANIFEST_PATH bridges/Cargo.toml
    CRATES simsapa_bridges
    LOCKED
    QMAKE ${qmake_path}

    QT_MODULES ${qt_modules}
)

cxx_qt_import_qml_module(simsapa_qml_module
    URI "com.profoundlabs.simsapa"
    SOURCE_CRATE simsapa_bridges)

qt_add_resources(icons "assets/icons.qrc")

list(APPEND cpp_files
    cpp/main.cpp
    cpp/errors.cpp
    cpp/utils.cpp
    cpp/screen.cpp
    cpp/android_helpers.cpp
    cpp/system_palette.cpp
    cpp/clipboard_manager.cpp
    cpp/gui.cpp
    cpp/window_manager.cpp
    cpp/sutta_search_window.cpp
    cpp/download_appdata_window.cpp
    cpp/sutta_languages_window.cpp
    cpp/dictionaries_window.cpp
    cpp/library_window.cpp
    cpp/reference_search_window.cpp
    cpp/topic_index_window.cpp
    cpp/chanting_practice_window.cpp
    cpp/chanting_review_window.cpp
    cpp/global_hotkey_manager.cpp)

# Linux X11 backend for the global hotkey manager. Detect the X11 dev libs
# via pkg-config; if present, define WITH_X11 and pull in the X11 / Xtst
# (XRecord) libraries. Skip on Android (no X11) and other non-Linux
# platforms. The X11-specific source compiles to an empty TU when WITH_X11
# is not defined.
if(UNIX AND NOT APPLE AND NOT ANDROID)
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(X11 QUIET x11 xtst)
    endif()
    if(X11_FOUND)
        list(APPEND cpp_files cpp/global_hotkey_x11.cpp)
        list(APPEND cpp_files cpp/window_activate_x11.cpp)
        message(STATUS "GlobalHotkeyManager: X11 backend enabled")
    else()
        message(STATUS "GlobalHotkeyManager: X11 dev libs not found; X11 backend disabled")
    endif()
endif()

# Windows backend for the global hotkey manager. Uses RegisterHotKey +
# SendInput; no extra link libraries beyond user32 (linked by default).
if(WIN32)
    list(APPEND cpp_files cpp/global_hotkey_win.cpp)
    message(STATUS "GlobalHotkeyManager: Windows backend enabled")
endif()

# macOS backend for the global hotkey manager. Uses Carbon RegisterEventHotKey
# + the Accessibility API + CGEventPost; links Carbon, AppKit, and
# ApplicationServices (see the target_link_libraries block further down).
if(APPLE AND NOT IOS)
    list(APPEND cpp_files cpp/global_hotkey_mac.mm)
    list(APPEND cpp_files cpp/window_activate_mac.mm)
    message(STATUS "GlobalHotkeyManager: macOS backend enabled")
endif()

list(APPEND app_files ${cpp_files} ${icons})

if (WIN32)
    set(app_icon_resource_windows "${CMAKE_CURRENT_SOURCE_DIR}/assets/icons/appicons/simsapa.rc")

    qt_add_executable(simsapadhammareader
        ${cpp_files}
        ${icons}
        ${app_icon_resource_windows})

elseif (APPLE)
    # The MACOSX_BUNDLE_ICON_FILE variable is added to the Info.plist
    # generated by CMake. This variable contains the .icns file name,
    # without the path.
    set(MACOSX_BUNDLE_ICON_FILE simsapa.icns)

    # And the following tells CMake where to find and install the file itself.
    set(app_icon_macos "${CMAKE_CURRENT_SOURCE_DIR}/assets/icons/appicons/simsapa.icns")
    set_source_files_properties(${app_icon_macos} PROPERTIES MACOSX_PACKAGE_LOCATION "Resources")

    qt_add_executable(simsapadhammareader MACOSX_BUNDLE ${app_files} ${app_icon_macos})

    set_target_properties(simsapadhammareader PROPERTIES
        MACOSX_BUNDLE_GUI_IDENTIFIER "io.github.simsapa.app"
    )

    # Patch NSMicrophoneUsageDescription into the CMake-generated Info.plist
    # after build. Required for macOS to show the microphone permission dialog.
    add_custom_command(TARGET simsapadhammareader POST_BUILD
        COMMAND /usr/libexec/PlistBuddy
            -c "Add :NSMicrophoneUsageDescription string 'Simsapa needs microphone access for voice recording features.'"
            "$<TARGET_BUNDLE_CONTENT_DIR:simsapadhammareader>/Info.plist"
            || /usr/libexec/PlistBuddy
            -c "Set :NSMicrophoneUsageDescription 'Simsapa needs microphone access for voice recording features.'"
            "$<TARGET_BUNDLE_CONTENT_DIR:simsapadhammareader>/Info.plist"
        VERBATIM
    )

elseif (ANDROID)
    list(APPEND android_files
        android/AndroidManifest.xml
        android/build.gradle
        android/gradle.properties
        android/gradlew
        android/gradlew.bat
        android/gradle/wrapper/gradle-wrapper.jar
        android/gradle/wrapper/gradle-wrapper.properties
        android/res/values/libs.xml
        android/res/xml/network_security_config.xml
        android/res/xml/qtprovider_paths.xml)

    qt_add_executable(simsapadhammareader ${app_files} ${android_files})

    # 16 KB page alignment (required for Play Store, targetSdk >= 35). Qt's own
    # prebuilt libs are already 16 KB-aligned, and the pure-Rust audio stack
    # (cpal AAudio backend) bundles no native libs, so the main app .so is the
    # only artifact we link here that needs aligning. NDK r28 would make this the
    # default, but it is incompatible with Qt 6.9.3 at minSdk 27 (libc++ uses
    # pthread_cond_clockwait, API 30+ only), so we stay on the Qt-supported NDK
    # (r26b/r27) and set max-page-size explicitly. See docs/pure-rust-audio-backend.md.
    target_link_options(simsapadhammareader PRIVATE "-Wl,-z,max-page-size=16384")

    # NDK system libraries needed by the Rust audio stack. cpal's AAudio backend
    # and the `ndk`/`ndk-sys` crates declare these via #[link(...)] attributes,
    # but those are only honoured when rustc drives the final link. Here CMake
    # links the staticlib (libsimsapa_bridges.a) into the app .so, so the
    # libraries must be named explicitly or the link fails with undefined
    # symbols (AAudio_*, ASharedMemory_*, ANativeWindow_*, ALooper_*, …).
    #   android       → ASharedMemory_*, ATrace_*, AInputQueue_*, ALooper_*
    #   nativewindow  → ANativeWindow_*
    #   aaudio        → cpal AAudio capture/playback
    # See docs/pure-rust-audio-backend.md.
    target_link_libraries(simsapadhammareader PRIVATE android nativewindow aaudio)

else()
    qt_add_executable(simsapadhammareader ${app_files})

endif()

set_target_properties(simsapadhammareader PROPERTIES
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
    QT_ANDROID_PACKAGE_SOURCE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/android
)

if (ANDROID)
    set_target_properties(simsapadhammareader PROPERTIES
        QT_ANDROID_VERSION_CODE "${ANDROID_VERSION_CODE}"
        QT_ANDROID_VERSION_NAME "${ANDROID_VERSION_NAME}"
    )
endif()

# Link to the qml module, which in turn links to the Rust simsapa library
target_link_libraries(simsapadhammareader
    PRIVATE
        simsapa_qml_module
    PUBLIC
        simsapa_bridges
        ${qt_modules}
    )

# Link SystemConfiguration framework on macOS (required by Rust dependencies),
# plus Carbon / AppKit / ApplicationServices for the global hotkey backend
# (RegisterEventHotKey, [NSWorkspace openURL:], AXIsProcessTrusted, etc.).
if(APPLE AND NOT IOS)
    target_link_libraries(simsapadhammareader
        PUBLIC
            "-framework SystemConfiguration"
            "-framework Carbon"
            "-framework AppKit"
            "-framework ApplicationServices"
    )
endif()

# Linux X11 hotkey backend: link X11 + Xtst (XRecord lives in libXtst).
if(UNIX AND NOT APPLE AND NOT ANDROID AND X11_FOUND)
    target_compile_definitions(simsapadhammareader PRIVATE WITH_X11)
    target_include_directories(simsapadhammareader PRIVATE ${X11_INCLUDE_DIRS})
    target_link_libraries(simsapadhammareader PUBLIC ${X11_LIBRARIES})
endif()

# Linux audio capture/playback: cpal's ALSA backend (in the Rust staticlib)
# references libasound symbols that must be resolved at the final link.
if(UNIX AND NOT APPLE AND NOT ANDROID)
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(ALSA QUIET alsa)
    endif()
    if(ALSA_FOUND)
        target_link_libraries(simsapadhammareader PUBLIC ${ALSA_LIBRARIES})
        message(STATUS "Audio: ALSA backend linked (${ALSA_LIBRARIES})")
    else()
        target_link_libraries(simsapadhammareader PUBLIC asound)
        message(STATUS "Audio: linking libasound directly (pkg-config alsa not found)")
    endif()
endif()

# Link bcrypt library on Windows (required by Rust getrandom crate)
if(WIN32)
    target_link_libraries(simsapadhammareader
        PUBLIC
            bcrypt
    )
endif()

# If we are using a statically linked Qt then we need to import any qml plugins
qt_import_qml_plugins(simsapadhammareader)

# On macOS, explicitly import the Darwin permission plugins for static Qt builds.
# Without this, QMicrophonePermission always returns Undetermined on static builds.
if (APPLE AND NOT IOS)
    qt_import_plugins(simsapadhammareader
        INCLUDE Qt6::QDarwinMicrophonePermissionPlugin
    )
endif()
