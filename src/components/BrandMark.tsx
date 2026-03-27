import { useCallback, useState } from 'react';
import brandSvgUrl from '@/assets/brand-icon.svg?url';

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
      setSrc(brandSvgUrl);
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
