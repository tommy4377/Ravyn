import { beforeEach, describe, expect, it } from "vitest";
import { navigation } from "./navigation.svelte";

describe("settings navigation guard", () => {
  beforeEach(() => {
    navigation.section = "downloads";
    navigation.settingsDirty = false;
    navigation.pendingSection = null;
    navigation.pendingAddKind = null;
    navigation.basketDrawerOpen = false;
    navigation.notificationDrawerOpen = false;
    navigation.navigationOverlayOpen = false;
  });

  it("blocks leaving Settings while backend changes are dirty", () => {
    navigation.section = "settings";
    navigation.settingsDirty = true;

    expect(navigation.navigate("library")).toBe(false);
    expect(navigation.section).toBe("settings");
    expect(navigation.pendingSection).toBe("library");
  });

  it("continues to the requested page after confirmation", () => {
    navigation.section = "settings";
    navigation.settingsDirty = true;
    navigation.navigate("automation");

    navigation.confirmPendingNavigation();

    expect(navigation.section).toBe("automation");
    expect(navigation.settingsDirty).toBe(false);
    expect(navigation.pendingSection).toBeNull();
  });

  it("keeps an Add request pending until the user confirms navigation", () => {
    navigation.section = "settings";
    navigation.settingsDirty = true;

    navigation.requestAdd("media");

    expect(navigation.section).toBe("settings");
    expect(navigation.pendingSection).toBe("downloads");
    expect(navigation.pendingAddKind).toBe("media");
  });
  it("keeps only one right-side drawer open", () => {
    navigation.openBasket();
    expect(navigation.basketDrawerOpen).toBe(true);
    expect(navigation.notificationDrawerOpen).toBe(false);

    navigation.openNotifications();
    expect(navigation.basketDrawerOpen).toBe(false);
    expect(navigation.notificationDrawerOpen).toBe(true);
  });

  it("closes notification history before other transient layers", () => {
    navigation.navigationOverlayOpen = true;
    navigation.notificationDrawerOpen = true;

    expect(navigation.closeTransientLayers()).toBe(true);
    expect(navigation.notificationDrawerOpen).toBe(false);
    expect(navigation.navigationOverlayOpen).toBe(true);
  });

});
