import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Animated,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { radii, spacing, type ThemeColors } from "../theme";
import { useTheme } from "../theme/ThemeContext";
import { Icon } from "./Icon";

type Props = {
  onUpload: () => void;
  onFolder: () => void;
  onDocument: () => void;
  onSpreadsheet: () => void;
  /** When false, hide + and camera (list scrolled away from top). */
  visible?: boolean;
};

export function CreateFab({
  onUpload,
  onFolder,
  onDocument,
  onSpreadsheet,
  visible = true,
}: Props) {
  const { colors } = useTheme();
  const styles = useMemo(() => makeStyles(colors), [colors]);
  const [open, setOpen] = useState(false);
  const insets = useSafeAreaInsets();
  const anim = useRef(new Animated.Value(visible ? 1 : 0)).current;

  useEffect(() => {
    if (!visible) setOpen(false);
    Animated.timing(anim, {
      toValue: visible ? 1 : 0,
      duration: 180,
      useNativeDriver: true,
    }).start();
  }, [visible, anim]);

  const close = () => setOpen(false);

  return (
    <View
      pointerEvents={visible ? "box-none" : "none"}
      style={styles.wrap}
    >
      {open ? (
        <Pressable style={styles.overlay} onPress={close} accessibilityLabel="Dismiss" />
      ) : null}

      <Animated.View
        style={[
          styles.stack,
          {
            paddingBottom: insets.bottom + spacing.lg,
            opacity: anim,
            transform: [
              {
                translateY: anim.interpolate({
                  inputRange: [0, 1],
                  outputRange: [24, 0],
                }),
              },
            ],
          },
        ]}
        pointerEvents={visible ? "box-none" : "none"}
      >
        <Pressable
          style={styles.camera}
          onPress={() => {
            /* Camera not wired yet */
          }}
          accessibilityLabel="Camera"
        >
          <Icon name="camera" size={22} color={colors.text} />
        </Pressable>

        {open ? (
          <>
            <Pressable
              style={styles.pill}
              onPress={() => {
                close();
                onDocument();
              }}
            >
              <Icon name="doc" size={20} color={colors.text} />
              <Text style={styles.pillText}>Document</Text>
            </Pressable>
            <Pressable
              style={styles.pill}
              onPress={() => {
                close();
                onSpreadsheet();
              }}
            >
              <Icon name="sheet" size={20} color={colors.text} />
              <Text style={styles.pillText}>Spreadsheet</Text>
            </Pressable>
            <Pressable
              style={styles.pill}
              onPress={() => {
                close();
                onUpload();
              }}
            >
              <Icon name="upload" size={20} color={colors.text} />
              <Text style={styles.pillText}>Upload</Text>
            </Pressable>
            <Pressable
              style={styles.pill}
              onPress={() => {
                close();
                onFolder();
              }}
            >
              <Icon name="folder" size={20} color={colors.text} />
              <Text style={styles.pillText}>Folder</Text>
            </Pressable>
          </>
        ) : null}

        <Pressable
          style={[styles.main, open && styles.mainOpen]}
          onPress={() => setOpen((v) => !v)}
          accessibilityLabel={open ? "Close" : "Create"}
        >
          <Icon
            name={open ? "close" : "plus"}
            size={28}
            color={open ? colors.bg : colors.text}
          />
        </Pressable>
      </Animated.View>
    </View>
  );
}

function makeStyles(colors: ThemeColors) {
  return StyleSheet.create({
    wrap: {
      ...StyleSheet.absoluteFill,
      justifyContent: "flex-end",
      alignItems: "flex-end",
      zIndex: 40,
    },
    overlay: {
      ...StyleSheet.absoluteFill,
      backgroundColor: colors.overlay,
    },
    stack: {
      paddingRight: spacing.lg,
      alignItems: "flex-end",
      gap: spacing.md,
    },
    camera: {
      width: 48,
      height: 48,
      borderRadius: 14,
      backgroundColor: colors.surfaceElevated,
      alignItems: "center",
      justifyContent: "center",
    },
    main: {
      width: 60,
      height: 60,
      borderRadius: 18,
      backgroundColor: colors.fab,
      alignItems: "center",
      justifyContent: "center",
    },
    mainOpen: {
      backgroundColor: colors.accentSoft,
    },
    pill: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      backgroundColor: colors.surfaceElevated,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.lg,
      borderRadius: radii.pill,
    },
    pillText: {
      color: colors.text,
      fontWeight: "600",
      fontSize: 15,
    },
  });
}
