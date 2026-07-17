import { message } from "../../shared/i18n";

const ROOT = "ravyn-root";

export const MenuId = {
  root: ROOT,
  linkDownload: "ravyn-link-download",
  linkPaused: "ravyn-link-paused",
  linkAnalyze: "ravyn-link-analyze",
  linkSchedule: "ravyn-link-schedule",
  linkScanPage: "ravyn-link-scan-page",
  imageDownload: "ravyn-image-download",
  imageOriginal: "ravyn-image-original",
  imageChoose: "ravyn-image-choose",
  imageConvert: "ravyn-image-convert",
  imageAll: "ravyn-image-all",
  mediaDownload: "ravyn-media-download",
  mediaAnalyze: "ravyn-media-analyze",
  mediaAudio: "ravyn-media-audio",
  mediaSubtitles: "ravyn-media-subtitles",
  mediaPicker: "ravyn-media-picker",
  selectionUrls: "ravyn-selection-urls",
  selectionScan: "ravyn-selection-scan",
  pageScan: "ravyn-page-scan",
  pageImages: "ravyn-page-images",
  pageMedia: "ravyn-page-media",
  pageYtdlp: "ravyn-page-ytdlp",
  pageMonitor: "ravyn-page-monitor",
  pagePopup: "ravyn-page-popup",
} as const;

export async function registerMenus(): Promise<void> {
  await browser.menus.removeAll();
  browser.menus.create({ id: ROOT, title: "Ravyn", contexts: ["all"] });
  create(MenuId.linkDownload, message("menuLinkDownload"), ["link"]);
  create(MenuId.linkPaused, message("menuLinkPaused"), ["link"]);
  create(MenuId.linkAnalyze, message("menuLinkAnalyze"), ["link"]);
  create(MenuId.linkSchedule, message("menuLinkSchedule"), ["link"]);
  create(MenuId.linkScanPage, message("menuLinkScanPage"), ["link"]);
  create(MenuId.imageDownload, message("menuImageDownload"), ["image"]);
  create(MenuId.imageOriginal, message("menuImageOriginal"), ["image"]);
  create(MenuId.imageChoose, message("menuImageChoose"), ["image"]);
  create(MenuId.imageConvert, message("menuImageConvert"), ["image"]);
  create(MenuId.imageAll, message("menuImageAll"), ["image"]);
  create(MenuId.mediaDownload, message("menuMediaDownload"), [
    "video",
    "audio",
  ]);
  create(MenuId.mediaAnalyze, message("menuMediaAnalyze"), ["video", "audio"]);
  create(MenuId.mediaAudio, message("menuMediaAudio"), ["video", "audio"]);
  create(MenuId.mediaSubtitles, message("menuMediaSubtitles"), ["video"]);
  create(MenuId.mediaPicker, message("menuMediaPicker"), ["video", "audio"]);
  create(MenuId.selectionUrls, message("menuSelectionUrls"), ["selection"]);
  create(MenuId.selectionScan, message("menuSelectionScan"), ["selection"]);
  create(MenuId.pageScan, message("menuPageScan"), ["page"]);
  create(MenuId.pageImages, message("menuPageImages"), ["page"]);
  create(MenuId.pageMedia, message("menuPageMedia"), ["page"]);
  create(MenuId.pageYtdlp, message("menuPageYtdlp"), ["page"]);
  create(MenuId.pageMonitor, message("menuPageMonitor"), ["page"]);
  create(MenuId.pagePopup, message("menuPagePopup"), ["page"]);
}

function create(
  id: string,
  title: string,
  contexts: browser.menus.ContextType[],
): void {
  browser.menus.create({ id, parentId: ROOT, title, contexts });
}
