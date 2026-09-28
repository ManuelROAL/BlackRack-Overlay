import { invoke } from "@tauri-apps/api/core";
import { readDisplayUnits } from "./display-units";
import { formatNumber, getLocale, t } from "./i18n";

/** One stored track and car combination, as `list_lap_records` returns it. */
interface LapRecordSummary {
  key: string;
  track: string;
  vehicle: string;
  track_length_meters: number;
  best_lap_seconds: number | null;
  optimal_lap_seconds: number | null;
  best_sector_seconds: [number | null, number | null, number | null];
  updated_unix_ms: number;
}

type ConfirmDeletion = (
  message: string,
  labels: { heading?: string; title?: string; confirmLabel?: string; danger?: boolean }
) => Promise<boolean>;

const formatLapTime = (seconds: number | null): string => {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return "--";
  const minutes = Math.floor(seconds / 60);
  const rest = (seconds % 60).toFixed(3);
  return minutes > 0 ? `${minutes}:${rest.padStart(6, "0")}` : rest;
};

const formatTrackLength = (meters: number): string =>
  readDisplayUnits().speed === "mph"
    ? `${formatNumber(meters / 1609.344, 2)} mi`
    : `${formatNumber(meters / 1000, 2)} km`;

const element = <K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string
): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
};

/**
 * Lists the lap references learned per track and car and lets the user delete
 * any of them. The data is read when the view opens, so the panel never polls
 * the database while the simulator is running.
 */
export const installLapRecordsPanel = (confirmDeletion: ConfirmDeletion): void => {
  const list = document.getElementById("lap-records-list");
  const status = document.getElementById("lap-records-status");
  const search = document.getElementById("lap-records-search") as HTMLInputElement | null;
  const refresh = document.getElementById("lap-records-refresh") as HTMLButtonElement | null;
  const section = list?.closest<HTMLElement>("[data-view-section]");
  if (!list || !status || !section) return;

  const dateFormatter = new Intl.DateTimeFormat(getLocale(), { dateStyle: "medium" });
  let records: LapRecordSummary[] = [];
  let loading = false;
  // The outcome of the last deletion, shown until the list changes again.
  let notice: string | null = null;

  const countText = (count: number): string =>
    count === 1 ? t("records.countOne") : t("records.count", { count });

  const deleteRecord = async (record: LapRecordSummary, button: HTMLButtonElement): Promise<void> => {
    const names = { track: record.track, vehicle: record.vehicle };
    if (!await confirmDeletion(t("records.deleteConfirm", names), {
      heading: t("records.deleteHeading"),
      title: t("records.deleteTitle"),
      confirmLabel: t("records.deleteAction"),
      danger: true
    })) return;
    button.disabled = true;
    try {
      await invoke("delete_lap_record", { key: record.key });
      records = records.filter(({ key }) => key !== record.key);
      notice = t("records.deleted", names);
    } catch {
      button.disabled = false;
      notice = t("records.deleteFailed");
    }
    render();
  };

  const recordRow = (record: LapRecordSummary): HTMLElement => {
    const row = element("div", "lap-record-row");
    row.setAttribute("role", "row");
    const vehicle = element("span", "lap-record-vehicle");
    vehicle.setAttribute("role", "cell");
    const updated = record.updated_unix_ms > 0 ? dateFormatter.format(record.updated_unix_ms) : "--";
    const vehicleName = element("strong", undefined, record.vehicle);
    vehicleName.title = record.vehicle;
    vehicle.append(
      vehicleName,
      element("small", undefined, `${t("records.updated")} ${updated}`)
    );
    row.append(vehicle);
    const cells: Array<[string, string]> = [
      ["lap-record-best", formatLapTime(record.best_lap_seconds)],
      ["lap-record-optimal", formatLapTime(record.optimal_lap_seconds)],
      ["lap-record-sectors", record.best_sector_seconds.map(formatLapTime).join(" · ")]
    ];
    for (const [className, text] of cells) {
      const cell = element("span", className, text);
      cell.setAttribute("role", "cell");
      row.append(cell);
    }
    const remove = element("button", "lap-record-delete", t("records.delete"));
    remove.type = "button";
    remove.setAttribute("aria-label", t("records.deleteAria", { track: record.track, vehicle: record.vehicle }));
    remove.addEventListener("click", () => void deleteRecord(record, remove));
    const actions = element("span", "lap-record-actions");
    actions.setAttribute("role", "cell");
    actions.append(remove);
    row.append(actions);
    return row;
  };

  const headerRow = (): HTMLElement => {
    const row = element("div", "lap-record-row lap-record-header");
    row.setAttribute("role", "row");
    for (const key of ["records.vehicle", "records.best", "records.optimal", "records.sectors"] as const) {
      const cell = element("span", undefined, t(key));
      cell.setAttribute("role", "columnheader");
      row.append(cell);
    }
    row.append(element("span"));
    return row;
  };

  const render = (): void => {
    const query = search?.value.trim().toLocaleLowerCase(getLocale()) ?? "";
    const visible = query
      ? records.filter(({ track, vehicle }) =>
        `${track} ${vehicle}`.toLocaleLowerCase(getLocale()).includes(query))
      : records;
    list.replaceChildren();
    status.textContent = notice ?? (records.length === 0
      ? t("records.empty")
      : visible.length === 0
        ? t("records.noMatch")
        : countText(records.length));

    // The same circuit name can exist in several layouts, so a group is one
    // name at one length.
    const groups = new Map<string, LapRecordSummary[]>();
    for (const record of visible) {
      const groupKey = `${record.track}\u001f${Math.round(record.track_length_meters)}`;
      groups.set(groupKey, [...groups.get(groupKey) ?? [], record]);
    }
    for (const entries of groups.values()) {
      const [first] = entries;
      const group = element("article", "lap-records-track");
      const heading = element("header");
      heading.append(
        element("strong", undefined, first.track),
        element("small", undefined, formatTrackLength(first.track_length_meters))
      );
      const table = element("div", "lap-records-table");
      table.setAttribute("role", "table");
      table.setAttribute("aria-label", first.track);
      table.append(headerRow(), ...entries.map(recordRow));
      group.append(heading, table);
      list.append(group);
    }
  };

  const load = async (): Promise<void> => {
    if (loading) return;
    loading = true;
    if (refresh) refresh.disabled = true;
    notice = null;
    if (records.length === 0) status.textContent = t("records.loading");
    try {
      records = await invoke<LapRecordSummary[]>("list_lap_records");
    } catch {
      records = [];
      notice = t("records.unavailable");
    }
    loading = false;
    if (refresh) refresh.disabled = false;
    render();
  };

  search?.addEventListener("input", () => {
    notice = null;
    render();
  });
  refresh?.addEventListener("click", () => void load());
  // Records change while driving, so the list is read again whenever the view
  // is opened rather than once at startup.
  document.querySelector(`[data-control-view="${section.dataset.viewSection}"]`)
    ?.addEventListener("click", () => void load());
  if (!section.hidden) void load();
};
