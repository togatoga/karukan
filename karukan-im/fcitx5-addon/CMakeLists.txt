cmake_minimum_required(VERSION 3.16)

project(fcitx5-karukan VERSION 0.1.0)


set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Find dependencies
find_package(ECM REQUIRED)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH} ${CMAKE_CURRENT_SOURCE_DIR}/cmake)

find_package(Fcitx5Core 5.0 REQUIRED)
find_package(Fcitx5Module COMPONENTS Notifications QuickPhrase)
find_package(PkgConfig REQUIRED)
pkg_check_modules(XKBCommon REQUIRED xkbcommon)

include(FeatureSummary)
include(GNUInstallDirs)
include(ECMSetupVersion)
include(ECMUninstallTarget)

# Use fcitx5 compiler settings
include("${FCITX_INSTALL_CMAKECONFIG_DIR}/Fcitx5Utils/Fcitx5CompilerSettings.cmake")

# Path to the Rust library
set(KARUKAN_RUST_LIB "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release/libkarukan_im.so")
set(KARUKAN_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../include")

# Build Rust library as a custom target
find_program(CARGO cargo REQUIRED)
add_custom_target(karukan_rust_lib ALL
    COMMAND ${CARGO} build --release -p karukan-im
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../.."
    COMMENT "Building karukan-im Rust library"
    BYPRODUCTS "${KARUKAN_RUST_LIB}"
)

# Add the addon
add_library(karukan MODULE
    src/karukan.cpp
)
add_dependencies(karukan karukan_rust_lib)

target_include_directories(karukan PRIVATE
    ${KARUKAN_INCLUDE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${XKBCommon_INCLUDE_DIRS}
)

target_link_libraries(karukan
    Fcitx5::Core
    Fcitx5::Config
    ${KARUKAN_RUST_LIB}
    ${XKBCommon_LIBRARIES}
)

# Remove lib prefix (fcitx5 expects "karukan.so" not "libkarukan.so")
set_target_properties(karukan PROPERTIES PREFIX "")

# Set RPATH so fcitx5 can find libkarukan_im.so at runtime
# $ORIGIN means "same directory as this library"
set_target_properties(karukan PROPERTIES
    BUILD_RPATH "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release"
    INSTALL_RPATH "\$ORIGIN"
    BUILD_WITH_INSTALL_RPATH FALSE
)

# Install the addon
install(TARGETS karukan DESTINATION "${FCITX_INSTALL_ADDONDIR}")

# Install Rust library alongside the addon
install(FILES "${KARUKAN_RUST_LIB}" DESTINATION "${FCITX_INSTALL_ADDONDIR}")

# Install addon config
install(FILES karukan-addon.conf.in DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon"
    RENAME karukan.conf)

# Install input method config
install(FILES karukan.conf DESTINATION "${FCITX_INSTALL_PKGDATADIR}/inputmethod")

# Install metainfo
install(FILES org.fcitx.Fcitx5.Addon.Karukan.metainfo.xml.in
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/metainfo"
    RENAME org.fcitx.Fcitx5.Addon.Karukan.metainfo.xml)

# Install icons at multiple sizes for XDG icon theme
install(FILES icons/fcitx-karukan-16.png
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/16x16/apps/"
    RENAME fcitx-karukan.png)
install(FILES icons/fcitx-karukan-24.png
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/24x24/apps/"
    RENAME fcitx-karukan.png)
install(FILES icons/fcitx-karukan-32.png
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/32x32/apps/"
    RENAME fcitx-karukan.png)
install(FILES icons/fcitx-karukan-48.png
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/48x48/apps/"
    RENAME fcitx-karukan.png)
install(FILES icons/fcitx-karukan-128.png
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/128x128/apps/"
    RENAME fcitx-karukan.png)

feature_summary(WHAT ALL FATAL_ON_MISSING_REQUIRED_PACKAGES)
