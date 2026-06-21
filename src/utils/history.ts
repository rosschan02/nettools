import { useCallback, useEffect, useState } from "react";

// 地址历史记录：用 localStorage 持久化用户输入过的地址，最近使用的排在最前。
// 每个工具用独立的 key（如 "ping.host"），互不干扰。

const STORAGE_PREFIX = "netools.history.";
const DEFAULT_MAX = 12;

function storageKey(key: string): string {
  return STORAGE_PREFIX + key;
}

function load(key: string): string[] {
  try {
    const raw = localStorage.getItem(storageKey(key));
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // 只保留非空字符串，防止历史数据被污染
    return parsed.filter((v): v is string => typeof v === "string" && v.length > 0);
  } catch {
    return [];
  }
}

function save(key: string, items: string[]): void {
  try {
    localStorage.setItem(storageKey(key), JSON.stringify(items));
  } catch {
    // localStorage 不可用（隐私模式 / 配额）时静默降级，不影响主流程
  }
}

export type AddressHistory = {
  items: string[];
  add: (value: string) => void;
  remove: (value: string) => void;
  clear: () => void;
};

export function useAddressHistory(key: string, max: number = DEFAULT_MAX): AddressHistory {
  const [items, setItems] = useState<string[]>(() => load(key));

  // 切换 key 时重新加载对应历史
  useEffect(() => {
    setItems(load(key));
  }, [key]);

  const add = useCallback(
    (raw: string) => {
      const value = raw.trim();
      if (!value) return;
      setItems((prev) => {
        // 去重后置顶，超出上限的丢弃
        const next = [value, ...prev.filter((v) => v !== value)].slice(0, max);
        save(key, next);
        return next;
      });
    },
    [key, max]
  );

  const remove = useCallback(
    (value: string) => {
      setItems((prev) => {
        const next = prev.filter((v) => v !== value);
        save(key, next);
        return next;
      });
    },
    [key]
  );

  const clear = useCallback(() => {
    setItems([]);
    save(key, []);
  }, [key]);

  return { items, add, remove, clear };
}
