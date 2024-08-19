[ -f "$HOME/export-esp.sh" ] && . "$HOME/export-esp.sh"

## TEST CODE FOR LVGL ##

# toolchain_dir=$(dirname $(command -v xtensa-esp32-elf-gcc))
# toolchain_dir=${toolchain_dir%/bin}
# toolchain_version=$(grep -o -E 'esp-([0-9.]+)_[0-9]+' <<< "$toolchain_dir")
# toolchain_version_code=${toolchain_version%_*}
# toolchain_version_code=${toolchain_version#esp-}

# export CROSS_COMPILE="xtensa-esp32-elf"
# C_INCLUDE_PATH="${toolchain_dir}/xtensa-esp-elf/include"
# C_INCLUDE_PATH="${C_INCLUDE_PATH}:${toolchain_dir}/xtensa-esp-elf/sys-include"
# C_INCLUDE_PATH="${C_INCLUDE_PATH}:${toolchain_dir}/lib/gcc/xtensa-esp-elf/${toolchain_version_code}/include"
# C_INCLUDE_PATH="${C_INCLUDE_PATH}:${toolchain_dir}/lib/gcc/xtensa-esp-elf/${toolchain_version_code}/include-fixed"
# export C_INCLUDE_PATH
