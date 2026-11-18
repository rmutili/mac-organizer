/** Folders the app can organize. The webview names a location; Rust resolves
 *  it to a path and checks it, and a "picked" folder comes from the native dialog. */
export type Location = "downloads" | "desktop" | "documents" | "picked";

export interface PickedFolder {
  name: string;
  /** For display only; it's never sent back to Rust. */
  path: string;
}

export const STANDARD_LOCATIONS: readonly { id: Exclude<Location, "picked">; label: string }[] = [
  { id: "downloads", label: "Downloads" },
  { id: "desktop", label: "Desktop" },
  { id: "documents", label: "Documents" },
];

export function locationLabel(location: Location, picked: PickedFolder | null): string {
  if (location === "picked") return picked?.name ?? "a folder";
  return STANDARD_LOCATIONS.find((l) => l.id === location)?.label ?? "a folder";
}
