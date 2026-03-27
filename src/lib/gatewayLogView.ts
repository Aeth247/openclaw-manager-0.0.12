/**
 * 将网关原始日志整理为「要点视图」：缩短时间、去掉长 UUID、压缩路径、合并重复行。
 */

const RE_UUID =
  /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

/** 任意位置的 ISO 时间 → 只保留时分秒（日志常为 `[gateway.log] 2026-03-27T08:...`） */
function shortenIsoTimestamps(line: string): string {
  return line.replace(
    /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?/g,
    (full) => {
      const m = full.match(/T(\d{2}:\d{2}:\d{2})/);
      return m ? m[1] : full;
    },
  );
}

/** Windows / Unix 用户目录路径 → ~ */
function shortenUserPaths(line: string): string {
  return line
    .replace(/[A-Za-z]:\\Users\\[^\\]+/gi, '~')
    .replace(/\/Users\/[^/]+/g, '~');
}

/** 保留 stderr 提示，stdout 去掉冗长文件名前缀 */
function shortenFileTag(line: string): string {
  return line
    .replace(/^\[gateway\.err\.log\]\s*/i, '⚠ ')
    .replace(/^\[[^\]]+\.log\]\s*/i, '');
}

function simplifyKnownPatterns(line: string): string | null {
  const lower = line.toLowerCase();

  if (/\[session-store\].*saved session/i.test(line)) {
    const m = line.match(/for\s+(\w+)\s*:/i);
    return m ? `会话已保存（${m[1]}）` : '会话已保存';
  }

  if (/gateway ready/i.test(line)) {
    const m = line.match(/\[([^\]]+)]\s*\[([^\]]+)]/);
    if (m) return `网关就绪 · ${m[2]}`;
    return '网关就绪';
  }

  if (/skipping startup greeting/i.test(line)) {
    return '已跳过启动问候（同版本）';
  }

  if (/webchat connected/i.test(line)) {
    const client = line.match(/client=([^\s]+)/i);
    const ver = line.match(/v?(\d{4}\.\d+\.\d+)/);
    const parts = [
      'Web 控制台已连接',
      client ? client[1] : null,
      ver ? ver[1] : null,
    ].filter(Boolean);
    return parts.join(' · ');
  }

  if (/plugins\.allow is empty/i.test(line)) {
    const m = line.match(/auto-load:\s*([^\s(]+)/i);
    const plug = m ? m[1].replace(/.*[\\/]/, '') : '扩展插件';
    return `配置提醒 · plugins.allow 未设置，已发现插件：${plug}（请在配置中显式允许受信任的 id）`;
  }

  if (/error|fatal|\berr\b/i.test(lower) && !/plugins\.allow/i.test(lower)) {
    // 保留错误类原意，只做通用清理（在后续步骤做 UUID/路径）
    return null;
  }

  return null;
}

function genericCleanup(line: string): string {
  let s = shortenIsoTimestamps(line);
  s = shortenUserPaths(s);
  s = s.replace(RE_UUID, '…');
  s = s.replace(/\bconn=[a-f0-9-]{10,}\b/gi, 'conn=…');
  s = s.replace(/\bsessionId=[^\s,]+/gi, 'sessionId=…');
  s = s.replace(/\blastSeq=\d+/gi, '');
  s = s.replace(/\s{2,}/g, ' ').trim();
  return shortenFileTag(s);
}

function dedupeRuns(lines: string[]): string[] {
  const out: string[] = [];
  let prev = '';
  let run = 1;
  const flush = () => {
    if (!prev) return;
    out.push(run > 1 ? `${prev} （同类重复 ${run} 条）` : prev);
  };
  for (const line of lines) {
    const t = line.trim();
    if (!t) continue;
    if (t === prev) {
      run += 1;
    } else {
      flush();
      prev = t;
      run = 1;
    }
  }
  flush();
  return out;
}

/**
 * @param rawLines get_logs 返回的原始行
 * @returns 适合界面展示的要点行
 */
export function simplifyGatewayLogs(rawLines: string[]): string[] {
  const mapped = rawLines.map((line) => {
    const friendly = simplifyKnownPatterns(line);
    if (friendly !== null) return genericCleanup(friendly);
    return genericCleanup(line);
  });
  return dedupeRuns(mapped);
}
