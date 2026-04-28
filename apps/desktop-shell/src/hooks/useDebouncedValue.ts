import { useEffect, useState } from "react";

/**
 * Return a debounced copy of `value`.
 *
 * Useful for search inputs where the backend should only be queried after
 * the user pauses typing for `delay` milliseconds.
 */
export function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);

  return debounced;
}
