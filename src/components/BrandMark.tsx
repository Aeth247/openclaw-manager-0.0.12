import { useCallback, useState } from 'react';

/** 打包后由 Vite 解析为实际 URL，避免 `*.svg?url` 在仅跑 tsc 时路径别名不匹配 */
const brandSvgFallbackUrl = new URL('../assets/brand-icon.svg', import.meta.url).href;

interface BrandMarkProps {
  className?: string;
  width?: number;
  height?: number;
}

/**
 * 品牌图：优先使用 `npm run icons:gen` 从 resources 同步的 `/brand-icon.png`（源自 exe_icon.png），
 * 缺失或加载失败时回退矢量图（与项目根目录 `resources/app-icon.svg` 经 icons:gen 同步到 src/assets）。
 */
export function BrandMark({ className, width = 32, height = 32 }: BrandMarkProps) {
  const [src, setSrc] = useState('/brand-icon.png');
  const [stage, setStage] = useState<'png' | 'svg'>('png');

  const onError = useCallback(() => {
    if (stage === 'png') {
      setStage('svg');
      setSrc(brandSvgFallbackUrl);
    }
  }, [stage]);

  return (
    <img
      src={src}
      alt=""
      width={width}
      height={height}
      className={className}
      onError={onError}
    />
  );
}
