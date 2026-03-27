/// <reference types="vite/client" />

/** Vite：以 URL 形式导入静态资源 */
declare module '*.svg?url' {
  const src: string;
  export default src;
}
