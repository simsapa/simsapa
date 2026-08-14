cmake_minimum_required(VERSION 3.24)

project(simsapadhammareader LANGUAGES CXX)

# Qt version per platform. Set these to match the installed Qt for each target.
# Used below to build CMAKE_PREFIX_PATH / qmake paths, so the version only needs
# to be changed in one place per platform. Everything else that needs a Qt
# version READS it from here -- the build scripts (via scripts/qt-env.sh), the
# Makefile and build-windows.ps1 -- so never hardcode a version a second time.
#
# ALL FIVE PLATFORMS ARE ON 6.9.3. Android was moved to 6.10.3 in August 2026
# and MOVED BACK after on-device testing; the per-platform variables stay
# because the divergence they allow is still the right mechanism -- it is the
# 6.10.3 target that failed, not the ability to diverge.
#
# Why the Android upgrade was reverted (measured on a Galaxy S23, Android 16,
# 2026-08-08 -- full account in docs/android-qt-upgrade-considerations.md):
#
#   * It did NOT fix the bug it was for. The Gboard/Thai mid-word Shift bug
#     behaves identically on 6.10.3. The PRD had inferred the fix from a
#     restartImmInput() call-site count in QtInputConnection.java (12 -> 2);
#     that inference was wrong. Removing the surviving per-keystroke call in
#     QtEditText.onKeyDown() (byte-identical in both versions) did not fix it
#     either. Gboard's own logs show it resetting its input context against
#     Qt's synthetic, empty editor -- no input-connection restart involved.
#     See docs/android-soft-keyboard.md section 4.
#   * It BROKE the Android UI. QtWebView was rearchitected in 6.10
#     (QQuickWebView moved from QQuickViewController to QQuickWindowContainer),
#     so the native WebView became a child window composited above the Qt
#     surface: it covers dialogs and dropdowns, ignores QML stacking, and takes
#     the InputConnection -- which broke text entry app-wide.
#
# Before raising any of these again, read
# docs/android-qt-upgrade-considerations.md: it records what to re-test, and
# that a Qt version must be judged on device, not by reading its sources.
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
        message(FATAL_ERROR "Qt6 ${QT_WINDOWS} not found in standard Windows locations. Please set CMAKE_PREFIX_PATH manually.")
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
        # Deliberately a WARNING, not FATAL_ERROR: this is a working fallback,
        # not a "Qt not found" condition. The build proceeds against the system
        # default SDK. Do not "harmonise" it with the two Qt-not-found sites.
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
            message(FATAL_ERROR "Qt6 ${QT_MACOS} not found in standard locations. Please set CMAKE_PREFIX_PATH manually.")
        endif()
    endif()
    message(STATUS "Using CMAKE_PREFIX_PATH: ${CMAKE_PREFIX_PATH}")

# Linux desktop. This branch MUST be guarded explicitly rather than written as a
# bare `else()`. In CMake, ANDROID is also UNIX-true, and the `if (ANDROID)`
# block further down is a SEPARATE top-level block, not a continuation of this
# chain -- so a bare `else()` here would catch Android, set CMAKE_PREFIX_PATH to
# the desktop Linux kit, and cause Android's own `if(NOT CMAKE_PREFIX_PATH)`
# guard to skip. The Android build would then link against desktop Qt: wrong
# version and wrong platform, while appearing to succeed.
# A bare `else()` would also catch Windows whenever CMAKE_PREFIX_PATH is preset.
elseif (UNIX AND NOT APPLE AND NOT ANDROID)
    # The inner guard is load-bearing: an explicitly-passed -DCMAKE_PREFIX_PATH
    # must win, and the multi-ABI Android sub-builds rely on the same pattern.
    if(NOT CMAKE_PREFIX_PATH)
        if(EXISTS "$ENV{HOME}/Qt/${QT_LINUX}/gcc_64")
            set(CMAKE_PREFIX_PATH "$ENV{HOME}/Qt/${QT_LINUX}/gcc_64")
        elseif(EXISTS "/opt/Qt/${QT_LINUX}/gcc_64")
            set(CMAKE_PREFIX_PATH "/opt/Qt/${QT_LINUX}/gcc_64")
        else()
            message(FATAL_ERROR
                "Qt ${QT_LINUX} not found under $ENV{HOME}/Qt/${QT_LINUX}/gcc_64 "
                "or /opt/Qt/${QT_LINUX}/gcc_64. Install it, or configure with "
                "-DCMAKE_PREFIX_PATH=<qt-dir>.")
        endif()
    endif()
    message(STATUS "Using CMAKE_PREFIX_PATH: ${CMAKE_PREFIX_PATH}")
endif()

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# This one list feeds BOTH rcc invocations in the project:
#   1. CMake's own AUTORCC, for assets/icons.qrc;
#   2. the bridge crate's, because cxx-qt-cmake 0.9.1 joins this list with ':'
#      and passes it to cargo as CXX_QT_AUTORCC_OPTIONS (cmake/CxxQt.cmake:99,111
#      -> corrosion_set_env_vars -> `cmake -E env VAR=VALUE cargo ...`).
#
# `--no-zstd` is the former cxx-qt fork patch A (Android: the shipped rcc must
# not emit zstd-compressed resources). Measured with Qt 6.9.3's rcc: given the
# `--compress-algo zlib` above, `--no-zstd` produces byte-identical output, so
# today it is belt-and-braces rather than the flag doing the work. It is kept
# because patch A carried it and because it keeps the guarantee if the
# compress-algo is ever changed. It is set here rather than exported from
# build-android.sh because corrosion's explicit `cmake -E env` assignment
# OVERRIDES the inherited environment -- a shell-exported CXX_QT_AUTORCC_OPTIONS
# is silently discarded on every CMake-driven build, which is all of them.
# Setting it here also propagates structurally to each per-ABI ExternalProject
# sub-build, which re-runs CMake over this same tree.
#
# ⚠ Changing this list does NOT trigger a Rust rebuild: cxx-qt-build reads the
# variable with env::var_os and emits no `cargo::rerun-if-env-changed`. Any
# experiment must `touch bridges/build.rs` or `cargo clean -p simsapa_bridges`
# first, or a stale rcc output is indistinguishable from a working change.
#
# See docs/cxx-qt-fork.md.
set(CMAKE_AUTORCC_OPTIONS --format-version 1)
list(APPEND CMAKE_AUTORCC_OPTIONS --compress-algo zlib)
list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)

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
    set(qt_expected_version "${QT_ANDROID}")
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
        # Default to arm64-v8a if ABI not detected.
        # Deliberately a WARNING, not FATAL_ERROR: this is a working fallback
        # with a sensible default, not a "Qt not found" condition.
        set(ANDROID_QT_DIR "$ENV{HOME}/Qt/${QT_ANDROID}/android_arm64_v8a")
        message(WARNING "ANDROID_ABI not detected, defaulting to android_arm64_v8a")
    endif()

    if(NOT CMAKE_PREFIX_PATH)
        set(CMAKE_PREFIX_PATH "${ANDROID_QT_DIR}")
    endif()
    message(STATUS "Android ABI: ${ANDROID_ABI}")
    message(STATUS "Using Android Qt: ${ANDROID_QT_DIR}")

    # Android's qmake is a POSIX shell wrapper applying the Android qt.conf --
    # it is NOT the host qmake6, and the name has no `6` suffix.
    set(qmake_exe_name "qmake")
    list(APPEND app_components WebView)
    list(APPEND qt_modules Qt::WebView)

    # Request CorePrivate EXPLICITLY. It is needed by cpp/android_raw_pick.cpp
    # (QtCore/private/qandroidextras_p.h) and is linked further down as
    # Qt6::CorePrivate.
    #
    # On 6.9.3 this line is redundant but correct, and it is kept deliberately:
    # 6.9.3's Qt6CoreConfig.cmake sets `__qt_Core_always_load_private_module ON`,
    # so `find_package(Qt6 COMPONENTS Core)` ALSO loads Qt6CorePrivate and
    # defines the target as a SIDE EFFECT -- which the project silently depended
    # on for as long as the private include has existed. Qt 6.10.3 sets that
    # variable OFF, and the 6.10.3 configure therefore failed outright with:
    #   Target "simsapadhammareader" links to: Qt6::CorePrivate
    #   but the target was not found.
    # (That was found during the reverted 6.10.3 attempt; asking explicitly costs
    # nothing here and removes the hidden dependency before the next upgrade.)
    # The alternative, QT_FIND_PRIVATE_MODULES, is far too broad -- it pulls in
    # every private module.
    #
    # Added to app_components, NOT to qt_modules: app_components is consumed
    # only by find_package, whereas qt_modules is also handed to
    # cxx_qt_import_crate(QT_MODULES), which resolves names through qmake and
    # has no business seeing a private module. See the comment at the
    # target_link_libraries call. Documented in docs/file-selection-test.md.
    list(APPEND app_components CorePrivate)

    add_compile_definitions(Q_OS_ANDROID)

    # Android package version. androiddeployqt substitutes these into
    # android/AndroidManifest.xml's %%INSERT_VERSION_CODE%% / %%INSERT_VERSION_NAME%%
    # placeholders. Google Play requires a strictly increasing versionCode on
    # every upload of a given package; bump it by editing android/version.txt,
    # then just:
    #   make android-aab
    #
    # build-android.sh reads android/version.txt and the [package] version in
    # bridges/Cargo.toml and exports both. Read from the ENVIRONMENT rather than
    # taking a CACHE variable or a -D argument: a cache entry is written once per
    # build directory, so an edited version.txt would be ignored on every
    # subsequent build in the same tree.
    #
    # Configure-time only. build-android.sh always re-runs `cmake -S . -B`, so
    # the release path always picks up an edit; a bare `cmake --build` or a Qt
    # Creator build reuses the previous configure and keeps the old value.
    set(ANDROID_VERSION_CODE "$ENV{ANDROID_VERSION_CODE}")
    set(ANDROID_VERSION_NAME "$ENV{ANDROID_VERSION_NAME}")
elseif (IOS)
    set(qt_expected_version "${QT_IOS}")
    set(Rust_CARGO_TARGET aarch64-apple-ios)
    set(qmake_exe_name "qmake6")
    list(APPEND app_components WebView)
    list(APPEND qt_modules Qt::WebView)
elseif (APPLE)
    set(qt_expected_version "${QT_MACOS}")
    # Explicitly set Rust target for macOS to prevent iOS cross-compilation
    if(CMAKE_HOST_SYSTEM_PROCESSOR MATCHES "arm64|aarch64")
        set(Rust_CARGO_TARGET aarch64-apple-darwin)
    else()
        set(Rust_CARGO_TARGET x86_64-apple-darwin)
    endif()

    set(qmake_exe_name "qmake6")
    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
elseif (WIN32)
    set(qt_expected_version "${QT_WINDOWS}")
    # Explicitly set Rust target for Windows MSVC
    set(Rust_CARGO_TARGET x86_64-pc-windows-msvc)

    # The three-way msvc2022_64 / msvc2019_64 / USERPROFILE search that used to
    # live here is gone: qmake_path is now derived from the Qt that
    # find_package() actually resolved (see below), so whichever of those
    # locations was found is followed automatically instead of being guessed at
    # a second time.
    #
    # Its bare `set(qmake_path "qmake6")` fallback is deliberately NOT replaced
    # by an equivalent. Falling back to whatever `qmake6` the ambient PATH holds
    # is the same class of defect as the missing Linux CMAKE_PREFIX_PATH branch:
    # it silently hands cxx-qt a different Qt than the C++ half is compiled
    # against. A missing qmake is now a FATAL_ERROR from the shared existence
    # check below.
    #
    # Do NOT derive this suffix from CMAKE_EXECUTABLE_SUFFIX -- when
    # cross-compiling that describes the target, not the host tool being run.
    set(qmake_exe_name "qmake6.exe")

    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
# Bare `else()` meaning "Linux desktop" is correct HERE, unlike in the
# CMAKE_PREFIX_PATH block above. This chain's FIRST branch is `if (ANDROID)`, so
# Android is already consumed and cannot fall through. The two blocks are
# therefore deliberately asymmetric -- do not "harmonise" them in either
# direction.
else()
    set(qt_expected_version "${QT_LINUX}")
    set(qmake_exe_name "qmake6")
    list(APPEND app_components WebEngineQuick)
    list(APPEND qt_modules Qt::WebEngineQuick)
endif()

find_package(Qt6 REQUIRED COMPONENTS ${CXXQT_QTCOMPONENTS} ${app_components})

# Assert the Qt that find_package() resolved is the one this project declares.
# Without this, an ambient PATH or CMAKE_PREFIX_PATH silently selects a
# different Qt than the QT_* variables above name -- which is exactly what was
# happening on Linux (system Qt was picked up while QT_LINUX said 6.9.3).
# This matters more now that Android and desktop deliberately target different
# Qt versions on the same machine.
if(NOT Qt6_VERSION VERSION_EQUAL "${qt_expected_version}")
    message(FATAL_ERROR
        "Qt version mismatch: expected ${qt_expected_version}, "
        "found ${Qt6_VERSION} at ${Qt6_DIR}")
endif()
message(STATUS "Qt ${Qt6_VERSION} (expected ${qt_expected_version}) at ${Qt6_DIR}")

# Derive qmake from the Qt that find_package() actually resolved, rather than
# rebuilding a path from the QT_* variables a second time.
#
# Why this is not just tidying: the per-platform branches above used to compute
# qmake_path independently of find_package(), so the two could disagree without
# anything noticing -- the C++/QML half compiled against one Qt while cxx-qt was
# handed another. That is precisely the mixed-version state measured on Linux
# (system Qt 6.11.1 for find_package, ~/Qt/6.9.3 for qmake). With the derivation
# below there is one Qt, and the assertion above has already vouched for it.
#
# Qt6_DIR is <prefix>/lib/cmake/Qt6, so three levels up is the kit prefix.
# Each branch above contributes only the executable's NAME, which genuinely does
# vary: Android's is a POSIX shell wrapper named `qmake`, Windows needs `.exe`,
# everything else is `qmake6`.
#
# Do NOT drop the QMAKE argument to cxx_qt_import_crate() below on the strength
# of this: cxx-qt-cmake's own fallback resolves Qt::qmake, which is the HOST
# qmake -- wrong for Android, whose kit qmake applies the Android qt.conf.
get_filename_component(_qt_prefix "${Qt6_DIR}/../../.." ABSOLUTE)
set(qmake_path "${_qt_prefix}/bin/${qmake_exe_name}")
if(NOT EXISTS "${qmake_path}")
    message(FATAL_ERROR
        "qmake not found at ${qmake_path} (derived from Qt6_DIR=${Qt6_DIR}). "
        "The Qt installation appears incomplete -- cxx-qt needs qmake to "
        "resolve Qt's include and library paths.")
endif()
message(STATUS "Using qmake: ${qmake_path}")

find_package(CxxQt QUIET)
if(NOT CxxQt_FOUND)
    include(FetchContent)
    FetchContent_Declare(
        CxxQt
        GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
        # This is the TAG `0.9.1`, deliberately, not the branch `0.9`. They point
        # at the same commit today, but kdab/cxx-qt-cmake publishes both --
        # refs/heads/0.9 alongside tags 0.9.0, 0.9.1 -- and FetchContent resolving
        # a branch re-resolves to whatever its latest commit is on every fresh
        # build. That is the same non-reproducibility bridges/Cargo.toml avoids by
        # pinning a rev; it was simply less visible here. The previous pin, `0.7`,
        # was the branch.
        #
        # cxx-qt-cmake and the Rust cxx-qt crates are a coupled pair -- a mismatch
        # fails inside generated code, the hardest place to read an error -- so
        # this must be bumped together with the four crate pins in
        # bridges/Cargo.toml, never on its own.
        GIT_TAG 0.9.1
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
    cpp/app_minimize.cpp
    cpp/android_helpers.cpp
    cpp/android_raw_pick.cpp
    cpp/system_palette.cpp
    cpp/clipboard_manager.cpp
    cpp/gui.cpp
    cpp/window_manager.cpp
    cpp/sutta_search_window.cpp
    cpp/download_appdata_window.cpp
    cpp/storage_recovery_window.cpp
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
    # The name under the launcher icon, and the title Google Play shows on the
    # install screen. androiddeployqt substitutes this into the
    # %%INSERT_APP_NAME%% placeholder in android/AndroidManifest.xml; with the
    # property unset it falls back to the CMake TARGET name, which shipped as
    # the unintended "simsapadhammareader".
    #
    # Set here rather than hardcoding android:label in the manifest, so the
    # androiddeployqt placeholder keeps working (unlike the permission/feature
    # markers, which were deliberately removed — see
    # docs/android-multi-abi-and-chromeos.md).
    set_target_properties(simsapadhammareader PROPERTIES
        QT_ANDROID_APP_NAME "Simsapa"
    )

    # Only set these when both values are actually present, so a developer
    # build configured without build-android.sh still succeeds — Qt then
    # defaults to versionCode 1 / versionName "1.0", which is fine for anything
    # that is not a Play upload.
    #
    # Tested with STREQUAL "" rather than if(DEFINED ...): the Makefile exports
    # both names unconditionally and GNU make exports an undefined variable as
    # the empty string, so on a plain `make android-aab` they arrive set-but-empty.
    if(NOT "${ANDROID_VERSION_CODE}" STREQUAL "" AND NOT "${ANDROID_VERSION_NAME}" STREQUAL "")
        set_target_properties(simsapadhammareader PROPERTIES
            QT_ANDROID_VERSION_CODE "${ANDROID_VERSION_CODE}"
            QT_ANDROID_VERSION_NAME "${ANDROID_VERSION_NAME}"
        )
        message(STATUS "Android package version: code ${ANDROID_VERSION_CODE}, name ${ANDROID_VERSION_NAME}")
    else()
        message(STATUS
            "ANDROID_VERSION_CODE / ANDROID_VERSION_NAME not set in the environment; "
            "Qt will default to versionCode 1 / versionName \"1.0\". "
            "Build with ./build-android.sh (or make android-aab) for a Play-ready package.")
    endif()
endif()

# Link to the qml module, which in turn links to the Rust simsapa library
target_link_libraries(simsapadhammareader
    PRIVATE
        simsapa_qml_module
    PUBLIC
        simsapa_bridges
        ${qt_modules}
    )

# The "File Selection Test" diagnostic reads the file picker's URI before Qt
# converts it, which needs QtCore/private/qandroidextras_p.h -- the same private
# API Qt's own Android file dialog uses. Deliberate and scoped; the decision and
# its evidence are in cpp/android_raw_pick.cpp and PRD §11 Q0a.
#
# Qt6::CorePrivate is an *interface* target: it contributes include paths, not a
# library, so this adds no .so to the package. Android only, and NOT added to
# ${qt_modules} -- that list is also handed to cxx_qt_import_crate(QT_MODULES),
# which resolves names through qmake and has no business seeing a private module.
if(ANDROID)
    target_link_libraries(simsapadhammareader PRIVATE Qt6::CorePrivate)
endif()

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
