/**
 * 与 Forever-helper「XC Cursor」一致的资源命名（resources/ 目录）：
 * - exe_icon.png    PyInstaller/Nuitka 里用作 exe 主图；此处优先作为 tauri icon 的输入
 * - window_icon.png 与 XC 的窗口图同名；仅当该文件不存在时才从 exe_icon 复制一份（你也可自备第二张图）
 * - app_icon.ico    XC / Inno 命名习惯；由 tauri 生成的 icon.ico 复制而来
 *
 * 若尚无 exe_icon.png，则退回到同目录下的 app-icon.svg。
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const tauriRes = path.join(root, 'src-tauri', 'resources');

function toPosix(p) {
  return p.split(path.sep).join('/');
}

function main() {
  fs.mkdirSync(tauriRes, { recursive: true });

  const exeIcon = path.join(tauriRes, 'exe_icon.png');
  const svg = path.join(tauriRes, 'app-icon.svg');

  let input;
  if (fs.existsSync(exeIcon)) {
    input = exeIcon;
  } else if (fs.existsSync(svg)) {
    input = svg;
  } else {
    throw new Error(
      '缺少图标源：请在 src-tauri/resources/ 放置 exe_icon.png（与 XC Cursor 的 resources/exe_icon.png 对齐）或 app-icon.svg'
    );
  }

  const relIn = toPosix(path.relative(root, input));
  const relOut = toPosix(path.relative(root, tauriRes));

  execSync(`npx tauri icon "${relIn}" -o "${relOut}"`, {
    stdio: 'inherit',
    cwd: root,
    shell: true,
  });

  const usedPngSource = path.extname(input).toLowerCase() === '.png';
  const iconPng = path.join(tauriRes, 'icon.png');
  if (!usedPngSource && fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, exeIcon);
  }

  const windowIcon = path.join(tauriRes, 'window_icon.png');
  if (fs.existsSync(exeIcon) && !fs.existsSync(windowIcon)) {
    fs.copyFileSync(exeIcon, windowIcon);
  }

  const ico = path.join(tauriRes, 'icon.ico');
  const appIco = path.join(tauriRes, 'app_icon.ico');
  if (fs.existsSync(ico)) {
    fs.copyFileSync(ico, appIco);
  }

  console.log(
    '[icons] 已按 XC Cursor 资源名同步：exe_icon.png、window_icon.png、app_icon.ico，以及 Tauri bundle 用 PNG/ICNS'
  );
}

main();
