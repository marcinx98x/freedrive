import React from "react";
import {
  ActivityIndicator,
  Image,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import type { NativeStackScreenProps } from "@react-navigation/native-stack";
import * as Sharing from "expo-sharing";
import { Pressable } from "react-native";
import type { RootStackParamList } from "../navigation/types";
import { colors, spacing } from "../theme";

type Props = NativeStackScreenProps<RootStackParamList, "FilePreview">;

export function FilePreviewScreen({ route, navigation }: Props) {
  const { title, uri, mime, mode, text } = route.params;

  React.useLayoutEffect(() => {
    navigation.setOptions({
      headerShown: true,
      title: title || "Preview",
      headerStyle: { backgroundColor: colors.bg },
      headerTintColor: colors.text,
      headerRight: () => (
        <Pressable
          onPress={async () => {
            if (await Sharing.isAvailableAsync()) {
              await Sharing.shareAsync(uri, { mimeType: mime, dialogTitle: title });
            }
          }}
          style={{ paddingHorizontal: 12 }}
        >
          <Text style={{ color: colors.accent, fontWeight: "600" }}>Share</Text>
        </Pressable>
      ),
    });
  }, [navigation, title, uri, mime]);

  if (mode === "image") {
    return (
      <View style={styles.center}>
        <Image source={{ uri }} style={styles.image} resizeMode="contain" />
      </View>
    );
  }

  if (mode === "text") {
    return (
      <ScrollView style={styles.safe} contentContainerStyle={styles.textPad}>
        <Text style={styles.text}>{text ?? ""}</Text>
      </ScrollView>
    );
  }

  // PDF / other: offer share open
  return (
    <View style={styles.center}>
      <Text style={styles.hint}>PDF preview uses your device apps.</Text>
      <Pressable
        style={styles.btn}
        onPress={async () => {
          if (await Sharing.isAvailableAsync()) {
            await Sharing.shareAsync(uri, { mimeType: mime || "application/pdf", dialogTitle: title });
          }
        }}
      >
        <Text style={styles.btnText}>Open with…</Text>
      </Pressable>
      <ActivityIndicator style={{ marginTop: 24 }} color={colors.accent} />
    </View>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
  center: {
    flex: 1,
    backgroundColor: colors.bg,
    alignItems: "center",
    justifyContent: "center",
    padding: spacing.lg,
  },
  image: { width: "100%", height: "100%" },
  textPad: { padding: spacing.lg },
  text: { color: colors.text, fontSize: 14, lineHeight: 20, fontFamily: "monospace" },
  hint: { color: colors.textSecondary, marginBottom: spacing.lg, textAlign: "center" },
  btn: {
    backgroundColor: colors.accentSoft,
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.md,
    borderRadius: 999,
  },
  btnText: { color: "#0B1C2C", fontWeight: "600" },
});
