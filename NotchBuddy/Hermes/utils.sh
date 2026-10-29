# Puts CPython and Hermes into the iPhone app as iOS wants them (project-hermes.yml runs this after the resources
# are copied). CPython's own iOS utilities (Apple/testbed/Python.xcframework/build/utils.sh in CPython's sources),
# with two changes: the frameworks are signed only when the build signs (the sideload build is signed later, by
# Sideloadly or AltStore, which sign every framework in the app), and the plist template is this folder's.
#
# iOS loads compiled code only from frameworks: every .so of the standard library and of the packages becomes a
# framework in the app's Frameworks folder, and a .fwork file where the .so was tells CPython's importer where it went.

install_stdlib() {
    PYTHON_XCFRAMEWORK_PATH=$1

    mkdir -p "$CODESIGNING_FOLDER_PATH/python/lib"
    if [ "$EFFECTIVE_PLATFORM_NAME" = "-iphonesimulator" ]; then
        echo "Installing Python modules for iOS Simulator"
        if [ -d "$PROJECT_DIR/$PYTHON_XCFRAMEWORK_PATH/ios-arm64-simulator" ]; then
            SLICE_FOLDER="ios-arm64-simulator"
        else
            SLICE_FOLDER="ios-arm64_x86_64-simulator"
        fi
    else
        echo "Installing Python modules for iOS Device"
        SLICE_FOLDER="ios-arm64"
    fi

    # A full XCframework has a shared lib folder and a slice-specific one; a single-slice one has only its own.
    if [ -d "$PROJECT_DIR/$PYTHON_XCFRAMEWORK_PATH/lib" ]; then
        rsync -au --delete "$PROJECT_DIR/$PYTHON_XCFRAMEWORK_PATH/lib/" "$CODESIGNING_FOLDER_PATH/python/lib/" --exclude 'libpython*.dylib'
        rsync -au "$PROJECT_DIR/$PYTHON_XCFRAMEWORK_PATH/$SLICE_FOLDER/lib-$ARCHS/" "$CODESIGNING_FOLDER_PATH/python/lib/" --exclude 'libpython*.dylib'
    else
        rsync -au --delete "$PROJECT_DIR/$PYTHON_XCFRAMEWORK_PATH/$SLICE_FOLDER/lib/" "$CODESIGNING_FOLDER_PATH/python/lib/" --exclude 'libpython*.dylib'
    fi
}

install_dylib () {
    PYTHON_XCFRAMEWORK_PATH=$1
    INSTALL_BASE=$2
    FULL_EXT=$3

    EXT=$(basename "$FULL_EXT")
    MODULE_PATH=$(dirname "$FULL_EXT")
    MODULE_NAME=$(echo $EXT | cut -d "." -f 1)
    RELATIVE_EXT=${FULL_EXT#$CODESIGNING_FOLDER_PATH/}
    PYTHON_EXT=${RELATIVE_EXT/$INSTALL_BASE/}
    FULL_MODULE_NAME=$(echo $PYTHON_EXT | cut -d "." -f 1 | tr "/" ".");
    FRAMEWORK_BUNDLE_ID=$(echo $PRODUCT_BUNDLE_IDENTIFIER.$FULL_MODULE_NAME | tr "_" "-")
    FRAMEWORK_FOLDER="Frameworks/$FULL_MODULE_NAME.framework"

    if [ ! -d "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER" ]; then
        echo "Creating framework for $RELATIVE_EXT"
        mkdir -p "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER"
        cp "$PROJECT_DIR/Hermes/iOS-dylib-Info-template.plist" "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/Info.plist"
        plutil -replace CFBundleExecutable -string "$FULL_MODULE_NAME" "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/Info.plist"
        plutil -replace CFBundleIdentifier -string "$FRAMEWORK_BUNDLE_ID" "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/Info.plist"
    fi

    echo "Installing binary for $FRAMEWORK_FOLDER/$FULL_MODULE_NAME"
    mv "$FULL_EXT" "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/$FULL_MODULE_NAME"
    echo "$FRAMEWORK_FOLDER/$FULL_MODULE_NAME" > ${FULL_EXT%.so}.fwork
    echo "${RELATIVE_EXT%.so}.fwork" > "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/$FULL_MODULE_NAME.origin"

    if [ -e "$MODULE_PATH/$MODULE_NAME.xcprivacy" ]; then
        XCPRIVACY_FILE="$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER/PrivacyInfo.xcprivacy"
        rm -rf "$XCPRIVACY_FILE"
        mv "$MODULE_PATH/$MODULE_NAME.xcprivacy" "$XCPRIVACY_FILE"
    fi

    if [ -n "${EXPANDED_CODE_SIGN_IDENTITY:-}" ] && [ "${CODE_SIGNING_ALLOWED:-YES}" != "NO" ]; then
        echo "Signing framework as $EXPANDED_CODE_SIGN_IDENTITY_NAME ($EXPANDED_CODE_SIGN_IDENTITY)..."
        /usr/bin/codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" ${OTHER_CODE_SIGN_FLAGS:-} -o runtime --timestamp=none --preserve-metadata=identifier,entitlements,flags --generate-entitlement-der "$CODESIGNING_FOLDER_PATH/$FRAMEWORK_FOLDER"
    fi
}

process_dylibs () {
    PYTHON_XCFRAMEWORK_PATH=$1
    LIB_PATH=$2
    find "$CODESIGNING_FOLDER_PATH/$LIB_PATH" -name "*.so" | while read FULL_EXT; do
        install_dylib $PYTHON_XCFRAMEWORK_PATH "$LIB_PATH/" "$FULL_EXT"
    done
}

install_python() {
    PYTHON_XCFRAMEWORK_PATH=$1
    shift

    install_stdlib $PYTHON_XCFRAMEWORK_PATH
    PYTHON_VER=$(ls -1 "$CODESIGNING_FOLDER_PATH/python/lib" | grep -E "^python3\.\d+$")
    echo "Install Python $PYTHON_VER standard library extension modules..."
    process_dylibs $PYTHON_XCFRAMEWORK_PATH python/lib/$PYTHON_VER/lib-dynload

    for package_path in $@; do
        echo "Installing $package_path extension modules ..."
        process_dylibs $PYTHON_XCFRAMEWORK_PATH $package_path
    done
}
