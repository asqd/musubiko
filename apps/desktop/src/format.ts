export function plural(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return one;
  if (m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20)) return few;
  return many;
}

/** "5 мин", "3 ч", "12 дн", "4 мес", "2 г" — short enough for a table column. */
export function ago(unix: number, now = Date.now() / 1000): string {
  const m = Math.max(0, Math.round((now - unix) / 60));
  if (m < 1) return "только что";
  if (m < 60) return `${m} мин`;
  if (m < 1440) return `${Math.round(m / 60)} ч`;
  if (m < 43200) return `${Math.round(m / 1440)} дн`;
  if (m < 525600) return `${Math.round(m / 43200)} мес`;
  return `${Math.round(m / 525600)} г`;
}

export function fullDate(unix: number): string {
  return new Date(unix * 1000).toLocaleString("ru-RU", { dateStyle: "medium", timeStyle: "short" });
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
