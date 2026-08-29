/** Định dạng số liệu sang chuỗi tiếng Việt dễ đọc. */

export function bytes(n: number, digits = 1): string {
  if (!n || n < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1);
  const v = n / Math.pow(1024, i);
  return `${v.toFixed(i === 0 ? 0 : digits).replace(".", ",")} ${units[i]}`;
}

export function speed(bps: number): string {
  return bps > 0 ? `${bytes(bps, 1)}/s` : "—";
}

export function duration(secs: number): string {
  if (!secs || secs <= 0) return "—";
  if (secs < 60) return `${Math.round(secs)} giây`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m} phút ${Math.round(secs % 60)} giây`;
  return `${Math.floor(m / 60)} giờ ${m % 60} phút`;
}

export function pct(n: number): string {
  return `${Math.min(100, Math.max(0, n)).toFixed(1).replace(".", ",")}%`;
}

/** Rút gọn đường dẫn dài, giữ lại tên file ở cuối. */
export function shortPath(p: string, max = 52): string {
  if (p.length <= max) return p;
  const parts = p.split(/[\\/]/);
  const file = parts[parts.length - 1] ?? p;
  return file.length >= max ? `…${file.slice(-(max - 1))}` : `…\\${file}`;
}
