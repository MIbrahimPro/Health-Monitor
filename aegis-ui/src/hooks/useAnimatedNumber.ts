import { useState, useEffect, useRef } from "react";

export function useAnimatedNumber(value: number | null, durationMs = 400): number | null {
  const [displayValue, setDisplayValue] = useState<number | null>(value);
  const targetRef = useRef<number | null>(value);
  const startRef = useRef<number | null>(null);
  const startTimeRef = useRef<number>(0);
  const requestRef = useRef<number>(0);

  useEffect(() => {
    if (value === null) {
      setDisplayValue(null);
      targetRef.current = null;
      return;
    }

    if (targetRef.current === null) {
      setDisplayValue(value);
      targetRef.current = value;
      return;
    }

    if (targetRef.current !== value) {
      startRef.current = displayValue ?? value;
      targetRef.current = value;
      startTimeRef.current = performance.now();

      const animate = (time: number) => {
        const elapsed = time - startTimeRef.current;
        const progress = Math.min(elapsed / durationMs, 1);
        const ease = 1 - Math.pow(1 - progress, 5); // easeOutQuint

        if (startRef.current !== null && targetRef.current !== null) {
          const current = startRef.current + (targetRef.current - startRef.current) * ease;
          setDisplayValue(current);
        }

        if (progress < 1) {
          requestRef.current = requestAnimationFrame(animate);
        }
      };

      cancelAnimationFrame(requestRef.current);
      requestRef.current = requestAnimationFrame(animate);
    }

    return () => cancelAnimationFrame(requestRef.current);
  }, [value, durationMs]);

  return displayValue !== null ? Math.round(displayValue) : null;
}
