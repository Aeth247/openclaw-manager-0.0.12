/**
 * 图标源目录（优先）：项目根目录 resources/
 *   D:\...\openclaw-manager-0.0.12\resources\exe_icon.png
 *   D:\...\openclaw-manager-0.0.12\resources\window_icon.png
 *   D:\...\openclaw-manager-0.0.12\resources\app-icon.svg
 * 兼容旧路径：src-tauri/resources/ 下同名文件
 *
 * 生成输出目录（固定）：src-tauri/resources/（tauri.conf 的 icon / bundle.resources 相对此目录）
 *
 * 替换图标后执行：npm run icons:gen，再 tauri build。
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const userRes = path.join(root, 'resources');
const tauriRes = path.join(root, 'src-tauri', 'resources');

function toPosix(p) {
  return p.split(path.sep).join('/');
}

function firstExisting(...paths) {
  for (const p of paths) {
    if (fs.existsSync(p)) return p;
  }
  return null;
}

function main() {
  fs.mkdirSync(userRes, { recursive: true });
  fs.mkdirSync(tauriRes, { recursive: true });

  const exeIconUser = path.join(userRes, 'exe_icon.png');
  const winIconUser = path.join(userRes, 'window_icon.png');
  const svgUser = path.join(userRes, 'app-icon.svg');
  const exeIconTauri = path.join(tauriRes, 'exe_icon.png');
  const winIconTauri = path.join(tauriRes, 'window_icon.png');
  const svgTauri = path.join(tauriRes, 'app-icon.svg');

  const exeIconOut = path.join(tauriRes, 'exe_icon.png');
  const windowIconOut = path.join(tauriRes, 'window_icon.png');

  const input = firstExisting(exeIconUser, winIconUser, svgUser, exeIconTauri, winIconTauri, svgTauri);

  if (!input) {
    throw new Error(
      '缺少图标：请在项目根目录 resources/ 放置 exe_icon.png、window_icon.png 或 app-icon.svg（亦可放在 src-tauri/resources/）'
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

  if (usedPngSource) {
    fs.copyFileSync(input, exeIconOut);
  } else if (fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, exeIconOut);
  }

  if (!fs.existsSync(windowIconOut) && fs.existsSync(exeIconOut)) {
    fs.copyFileSync(exeIconOut, windowIconOut);
  }

  if (fs.existsSync(winIconUser)) {
    fs.copyFileSync(winIconUser, windowIconOut);
  }

  const ico = path.join(tauriRes, 'icon.ico');
  const appIco = path.join(tauriRes, 'app_icon.ico');
  if (fs.existsSync(ico)) {
    fs.copyFileSync(ico, appIco);
  }

  const publicDir = path.join(root, 'public');
  fs.mkdirSync(publicDir, { recursive: true });
  const brandPub = path.join(publicDir, 'brand-icon.png');
  const winPub = path.join(publicDir, 'window-icon.png');

  if (fs.existsSync(exeIconOut)) {
    fs.copyFileSync(exeIconOut, brandPub);
  } else if (fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, brandPub);
  }

  if (fs.existsSync(windowIconOut)) {
    fs.copyFileSync(windowIconOut, winPub);
  } else if (fs.existsSync(exeIconOut)) {
    fs.copyFileSync(exeIconOut, winPub);
  } else if (fs.existsSync(iconPng)) {
    fs.copyFileSync(iconPng, winPub);
  }

  const svgSrc = firstExisting(svgUser, svgTauri);
  const webBrandSvg = path.join(root, 'src', 'assets', 'brand-icon.svg');
  if (svgSrc) {
    fs.mkdirSync(path.dirname(webBrandSvg), { recursive: true });
    fs.copyFileSync(svgSrc, webBrandSvg);
  }

  const pubSvg = path.join(publicDir, 'app-icon.svg');
  if (svgSrc) {
    fs.copyFileSync(svgSrc, pubSvg);
  }

  console.log(
    '[icons] 自 resources/（或 src-tauri/resources）生成套装 → src-tauri/resources，并同步 public/ 与 src/assets/brand-icon.svg'
  );
}

main();
