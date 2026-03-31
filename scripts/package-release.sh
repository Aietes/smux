#!/usr/bin/env bash

set -euo pipefail

version="${1:?missing version}"
target="${2:?missing target}"
binary_path="${3:?missing binary path}"
out_dir="${4:?missing output directory}"

stage_dir="${out_dir}/smux-${version}-${target}"
archive_path="${out_dir}/smux-${version}-${target}.tar.gz"

mkdir -p "${stage_dir}"
cp "${binary_path}" "${stage_dir}/smux"
cp README.md "${stage_dir}/README.md"
cp LICENSE "${stage_dir}/LICENSE"

tar -C "${out_dir}" -czf "${archive_path}" "smux-${version}-${target}"
rm -rf "${stage_dir}"
