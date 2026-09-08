import * as Network from "expo-network";
import { getWifiOnly } from "./prefs";

export class WifiRequiredError extends Error {
  constructor(message = "Waiting for Wi-Fi… Transfers are limited to Wi-Fi in Settings.") {
    super(message);
    this.name = "WifiRequiredError";
  }
}

/** Throws WifiRequiredError when Wi-Fi-only is on and the device is not on Wi-Fi. */
export async function assertTransferAllowed(): Promise<void> {
  const wifiOnly = await getWifiOnly();
  if (!wifiOnly) return;

  const state = await Network.getNetworkStateAsync();
  const type = state.type;
  if (
    type === Network.NetworkStateType.WIFI ||
    type === Network.NetworkStateType.ETHERNET
  ) {
    return;
  }

  throw new WifiRequiredError();
}
