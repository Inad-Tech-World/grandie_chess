import { useCallback, useEffect, useRef, useState } from "react";

/** A form field that shows edits at once but saves them only after typing
 *  pauses, so a backend write isn't made for every keystroke. A pending save
 *  is flushed when the field unmounts. */
export function useDebouncedField<T>(value: T, commit: (v: T) => void, ms = 400): [T, (v: T) => void] {
  const [local, setLocal] = useState(value);
  const pending = useRef<{ value: T } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const commitRef = useRef(commit);
  commitRef.current = commit;

  // Follow outside changes while nothing is waiting to be saved.
  useEffect(() => {
    if (!pending.current) setLocal(value);
  }, [value]);

  const flush = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const p = pending.current;
    pending.current = null;
    if (p) commitRef.current(p.value);
  }, []);

  useEffect(() => flush, [flush]);

  const set = useCallback(
    (v: T) => {
      setLocal(v);
      pending.current = { value: v };
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(flush, ms);
    },
    [flush, ms],
  );

  return [local, set];
}
