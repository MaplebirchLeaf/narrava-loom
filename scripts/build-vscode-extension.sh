#!/usr/bin/env bash
# 构建 Narrava Twee VS Code 扩展；可选地安装刚生成的精确版本。

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository="$(cd -- "$script_dir/.." && pwd)"
extension_dir="$repository/editors/vscode-narrava-loom"
output_dir="$repository/dist/vscode-narrava-loom"
install=false
editor="${NARRAVA_VSCODE_COMMAND:-code}"

usage() {
  cat <<'EOF'
用法：scripts/build-vscode-extension.sh [选项]

选项：
  --install          构建后安装刚生成的 .vsix
  --editor COMMAND   指定安装命令，例如 code、codium（默认：code）
  -h, --help         显示帮助

也可用 NARRAVA_VSCODE_COMMAND 设置默认编辑器命令。
EOF
}

while (($# > 0)); do
  case "$1" in
    --install)
      install=true
      ;;
    --editor)
      shift
      (($# > 0)) || { echo "--editor 缺少命令" >&2; exit 2; }
      editor="$1"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知选项：$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

command -v npm >/dev/null 2>&1 || { echo "找不到 npm" >&2; exit 1; }
command -v node >/dev/null 2>&1 || { echo "找不到 node" >&2; exit 1; }

package_name="$(node -p "require('$extension_dir/package.json').name")"
package_version="$(node -p "require('$extension_dir/package.json').version")"
vsix="$output_dir/$package_name-$package_version.vsix"

cd -- "$extension_dir"
if [[ ! -d node_modules ]]; then
  echo "安装扩展测试依赖……"
  npm ci
fi

echo "运行扩展回归测试……"
npm test
mkdir -p -- "$output_dir"

# 写入精确文件名，避免把旧版本误认成刚生成的产物。
echo "构建 $vsix ……"
npx --yes @vscode/vsce package --skip-license --allow-missing-repository --out "$vsix"

if [[ "$install" == true ]]; then
  command -v "$editor" >/dev/null 2>&1 || {
    echo "找不到编辑器命令：$editor" >&2
    exit 1
  }
  echo "安装 $vsix ……"
  "$editor" --install-extension "$vsix" --force
fi

echo "完成：$vsix"
