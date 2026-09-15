import { useCallback, useRef, useState } from "react";
import type { NativeScrollEvent, NativeSyntheticEvent } from "react-native";

/** Show FAB only while the list is at (or very near) the top — Google Drive style. */
const TOP_THRESHOLD = 12;

export function useFabScrollVisibility() {
  const [fabVisible, setFabVisible] = useState(true);
  const visibleRef = useRef(true);

  const onScroll = useCallback((e: NativeSyntheticEvent<NativeScrollEvent>) => {
    const y = e.nativeEvent.contentOffset.y;
    const next = y <= TOP_THRESHOLD;
    if (next === visibleRef.current) return;
    visibleRef.current = next;
    setFabVisible(next);
  }, []);

  const resetFabVisible = useCallback(() => {
    visibleRef.current = true;
    setFabVisible(true);
  }, []);

  return { fabVisible, onScroll, resetFabVisible };
}
