import type { Locale } from "./i18n";

export interface LocalizedReleaseNotes {
  updateTitle: string | null;
  fullTitle: string | null;
  changelog: string[];
}

export interface ReleaseNotesManifest {
  version: string;
  releasePageUrl: string | null;
  updateTitle: string | null;
  fullTitle: string | null;
  changelog: string[];
  localized?: Record<string, LocalizedReleaseNotes>;
}

const manifestModules = import.meta.glob<ReleaseNotesManifest>(
  "../release/*/manifest.json",
  { eager: true, import: "default" }
);

const versionParts = (version: string): number[] => version
  .split(".")
  .map((part) => Number.parseInt(part, 10));

const compareVersions = (left: string, right: string): number => {
  const leftParts = versionParts(left);
  const rightParts = versionParts(right);
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (rightParts[index] ?? 0) - (leftParts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
};

export const RELEASE_NOTES = Object.values(manifestModules)
  .filter((manifest) => manifest.changelog.length > 0)
  .sort((left, right) => compareVersions(left.version, right.version));

export const selectReleaseNotes = (
  manifest: ReleaseNotesManifest,
  locale: Locale
): Pick<ReleaseNotesManifest, "updateTitle" | "fullTitle" | "changelog"> => {
  const candidates = [locale, locale.split(/[-_]/, 1)[0], "en"]
    .filter((candidate, index, values) => values.indexOf(candidate) === index)
    .map((candidate) => manifest.localized?.[candidate])
    .filter((localized): localized is LocalizedReleaseNotes => localized !== undefined);
  const changelog = candidates.find((candidate) => candidate.changelog.length > 0)?.changelog;
  return {
    updateTitle: candidates.find((candidate) => candidate.updateTitle)?.updateTitle ?? manifest.updateTitle,
    fullTitle: candidates.find((candidate) => candidate.fullTitle)?.fullTitle ?? manifest.fullTitle,
    changelog: changelog ?? manifest.changelog
  };
};
