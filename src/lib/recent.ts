// The list of recent comparisons, remembered between launches.
export interface Recent {
  left: string;
  right: string;
  at: number;
}

const KEY = 'sbs-recent';
const MAX = 10;

export function loadRecent(): Recent[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(v)
      ? v.filter((x) => typeof x?.left === 'string' && typeof x?.right === 'string' && typeof x?.at === 'number')
      : [];
  } catch {
    return [];
  }
}

function save(list: Recent[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Not saved this time; the list still works until the app closes.
  }
}

export function addRecent(left: string, right: string): Recent[] {
  const list = [{ left, right, at: Date.now() }, ...loadRecent().filter((r) => r.left !== left || r.right !== right)].slice(0, MAX);
  save(list);
  return list;
}

export function removeRecent(item: Recent): Recent[] {
  const list = loadRecent().filter((r) => r.left !== item.left || r.right !== item.right);
  save(list);
  return list;
}

export function clearRecent(): Recent[] {
  save([]);
  return [];
}

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
export const folderOf = (p: string) => p.slice(0, Math.max(0, p.length - fileName(p).length - 1));

export function timeAgo(at: number): string {
  const s = Math.round((Date.now() - at) / 1000);
  if (s < 60) return 'just now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 7) return d === 1 ? 'yesterday' : `${d} days ago`;
  return new Date(at).toLocaleDateString();
}
