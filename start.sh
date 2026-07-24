#!/bin/bash
cd aegis-ui
env -u GTK_PATH -u LD_LIBRARY_PATH -u GIO_MODULE_DIR npm run tauri dev
