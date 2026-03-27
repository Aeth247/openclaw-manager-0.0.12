/**
 * 与 Forever-helper「XC Cursor」一致的资源命名（src-tauri/resources/）：
 * - exe_icon.png     主品牌图 → 生成 Tauri 全套 icon，并同步到前端 public/brand-icon.png
 * - window_icon.png  窗口/任务栏用小图；若不存在则从 exe_icon 复制；同步到 public/window-icon.png
 * - app_icon.ico     由 icon.ico 复制
 *
 * 生成源优先级：exe_icon.png → window_icon.png → app-icon.svg（不使用虾/爪素材）
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
  const windowIconPath = path.join(tauriRes, 'window_icon.png');
  const svg = path.join(tauriRes, 'app-icon.svg');

  let input;
  if (fs.existsSync(exeIcon)) {
    input = exeIcon;
  } else if (fs.existsSync(windowIconPath)) {
    input = windowIconPath;
  } else if (fs.existsSync(svg)) {
    input = svg;
  } else {
    throw new Error(
      '缺少图标：请在 src-tauri/resources/ 放置 exe_icon.png（与 window_icon.png），或至少提供 app-icon.svg'
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

  // 前端固定使用 PNG 路径，避免引用虾爪 SVG
  const publicDir = path.join(root, 'public');
  fs.mkdirSync(publicDir, { recursive: true });
  const brandPub = path.join(publicDir, 'brand-icon.png');
  const winPub = path.join(publicDir, 'window-icon.png');

  if (fs.existsSync(exeIcon)) {
    fs.copyFileSync(exeIcon, brandPub);
  } else if (fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, brandPub);
  }

  if (fs.existsSync(windowIcon)) {
    fs.copyFileSync(windowIcon, winPub);
  } else if (fs.existsSync(exeIcon)) {
    fs.copyFileSync(exeIcon, winPub);
  } else if (fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, winPub);
  }

  console.log(
    '[icons] 已同步 exe_icon / window_icon → bundle PNG/ICO/ICNS，并写入 public/brand-icon.png、public/window-icon.png'
  );
}

main();
