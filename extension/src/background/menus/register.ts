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
  create(MenuId.linkDownload, "Download link with Ravyn", ["link"]);
  create(MenuId.linkPaused, "Add link paused", ["link"]);
  create(MenuId.linkAnalyze, "Analyze link", ["link"]);
  create(MenuId.linkSchedule, "Schedule link in Ravyn", ["link"]);
  create(MenuId.linkScanPage, "Scan linked page", ["link"]);
  create(MenuId.imageDownload, "Download image with Ravyn", ["image"]);
  create(MenuId.imageOriginal, "Download original image", ["image"]);
  create(MenuId.imageChoose, "Choose image source", ["image"]);
  create(MenuId.imageConvert, "Convert image to WebP and download", ["image"]);
  create(MenuId.imageAll, "Download all page images", ["image"]);
  create(MenuId.mediaDownload, "Download media with Ravyn", ["video", "audio"]);
  create(MenuId.mediaAnalyze, "Analyze available formats", ["video", "audio"]);
  create(MenuId.mediaAudio, "Download audio only", ["video", "audio"]);
  create(MenuId.mediaSubtitles, "Download subtitles", ["video"]);
  create(MenuId.mediaPicker, "Open media picker", ["video", "audio"]);
  create(MenuId.selectionUrls, "Download URLs in selection", ["selection"]);
  create(MenuId.selectionScan, "Scan selection for links", ["selection"]);
  create(MenuId.pageScan, "Scan page resources", ["page"]);
  create(MenuId.pageImages, "Download all images", ["page"]);
  create(MenuId.pageMedia, "Download all media", ["page"]);
  create(MenuId.pageYtdlp, "Send page to yt-dlp", ["page"]);
  create(MenuId.pageMonitor, "Monitor page for new resources", ["page"]);
  create(MenuId.pagePopup, "Open Ravyn resource picker", ["page"]);
}

function create(
  id: string,
  title: string,
  contexts: browser.menus.ContextType[],
): void {
  browser.menus.create({ id, parentId: ROOT, title, contexts });
}
