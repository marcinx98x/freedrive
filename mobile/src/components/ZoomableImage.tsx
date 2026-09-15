import React, {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useCallback,
} from "react";
import { Image, StyleSheet, View } from "react-native";
import { Gesture, GestureDetector } from "react-native-gesture-handler";
import Animated, {
  runOnJS,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from "react-native-reanimated";

const MIN_SCALE = 1;
const MAX_SCALE = 4;
const DOUBLE_TAP_SCALE = 2.5;
const STEP = 0.25;

export type ZoomableImageHandle = {
  zoomIn: () => void;
  zoomOut: () => void;
  reset: () => void;
};

type Props = {
  uri: string;
  /** When false (off-screen gallery page), force reset to 1× */
  active?: boolean;
  onZoomChange?: (scale: number) => void;
};

export const ZoomableImage = forwardRef<ZoomableImageHandle, Props>(
  function ZoomableImage({ uri, active = true, onZoomChange }, ref) {
    const scale = useSharedValue(1);
    const savedScale = useSharedValue(1);
    const translateX = useSharedValue(0);
    const translateY = useSharedValue(0);
    const savedTX = useSharedValue(0);
    const savedTY = useSharedValue(0);

    const emitZoom = useCallback(
      (value: number) => {
        onZoomChange?.(value);
      },
      [onZoomChange],
    );

    const resetAll = useCallback(() => {
      scale.value = withTiming(1, { duration: 180 });
      savedScale.value = 1;
      translateX.value = withTiming(0, { duration: 180 });
      translateY.value = withTiming(0, { duration: 180 });
      savedTX.value = 0;
      savedTY.value = 0;
      emitZoom(1);
    }, [scale, savedScale, translateX, translateY, savedTX, savedTY, emitZoom]);

    useImperativeHandle(
      ref,
      () => ({
        zoomIn: () => {
          const next = Math.min(MAX_SCALE, savedScale.value + STEP);
          scale.value = withTiming(next, { duration: 160 });
          savedScale.value = next;
          emitZoom(next);
        },
        zoomOut: () => {
          const next = Math.max(MIN_SCALE, savedScale.value - STEP);
          scale.value = withTiming(next, { duration: 160 });
          savedScale.value = next;
          if (next <= 1.01) {
            translateX.value = withTiming(0, { duration: 160 });
            translateY.value = withTiming(0, { duration: 160 });
            savedTX.value = 0;
            savedTY.value = 0;
          }
          emitZoom(next);
        },
        reset: resetAll,
      }),
      [emitZoom, resetAll, scale, savedScale, translateX, translateY, savedTX, savedTY],
    );

    useEffect(() => {
      if (!active) {
        resetAll();
      }
    }, [active, resetAll]);

    useEffect(() => {
      resetAll();
    }, [uri, resetAll]);

    const pinch = Gesture.Pinch()
      .onStart(() => {
        savedScale.value = scale.value;
      })
      .onUpdate((e) => {
        const next = Math.max(
          MIN_SCALE,
          Math.min(MAX_SCALE, savedScale.value * e.scale),
        );
        scale.value = next;
      })
      .onEnd(() => {
        const next =
          scale.value < 1.05
            ? 1
            : Math.max(MIN_SCALE, Math.min(MAX_SCALE, scale.value));
        scale.value = withTiming(next, { duration: 140 });
        savedScale.value = next;
        if (next <= 1.01) {
          translateX.value = withTiming(0, { duration: 140 });
          translateY.value = withTiming(0, { duration: 140 });
          savedTX.value = 0;
          savedTY.value = 0;
        }
        runOnJS(emitZoom)(next);
      });

    const pan = Gesture.Pan()
      .averageTouches(true)
      .manualActivation(true)
      .onTouchesMove((_, state) => {
        // At 1× fail so horizontal FlatList paging keeps the swipe
        if (scale.value > 1.01) {
          state.activate();
        } else {
          state.fail();
        }
      })
      .onStart(() => {
        savedTX.value = translateX.value;
        savedTY.value = translateY.value;
      })
      .onUpdate((e) => {
        if (scale.value <= 1.01) return;
        translateX.value = savedTX.value + e.translationX;
        translateY.value = savedTY.value + e.translationY;
      })
      .onEnd(() => {
        savedTX.value = translateX.value;
        savedTY.value = translateY.value;
      });

    const doubleTap = Gesture.Tap()
      .numberOfTaps(2)
      .onEnd(() => {
        if (scale.value > 1.05) {
          scale.value = withTiming(1, { duration: 180 });
          savedScale.value = 1;
          translateX.value = withTiming(0, { duration: 180 });
          translateY.value = withTiming(0, { duration: 180 });
          savedTX.value = 0;
          savedTY.value = 0;
          runOnJS(emitZoom)(1);
        } else {
          scale.value = withTiming(DOUBLE_TAP_SCALE, { duration: 180 });
          savedScale.value = DOUBLE_TAP_SCALE;
          runOnJS(emitZoom)(DOUBLE_TAP_SCALE);
        }
      });

    const composed = Gesture.Simultaneous(
      pinch,
      Gesture.Exclusive(doubleTap, pan),
    );

    const animatedStyle = useAnimatedStyle(() => ({
      transform: [
        { translateX: translateX.value },
        { translateY: translateY.value },
        { scale: scale.value },
      ],
    }));

    return (
      <View style={styles.fill} collapsable={false}>
        <GestureDetector gesture={composed}>
          <Animated.View style={[styles.fill, animatedStyle]}>
            <Image source={{ uri }} style={styles.image} resizeMode="contain" />
          </Animated.View>
        </GestureDetector>
      </View>
    );
  },
);

const styles = StyleSheet.create({
  fill: { flex: 1, width: "100%", height: "100%" },
  image: { width: "100%", height: "100%" },
});
