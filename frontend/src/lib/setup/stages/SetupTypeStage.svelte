<script lang="ts">
  import { untrack } from "svelte";
  import StageShell from "../StageShell.svelte";
  import RadioGroup from "../../components/RadioGroup.svelte";
  import type { SetupController } from "../controller.svelte";
  import type { SetupProfile } from "../../api/types";

  let { controller }: { controller: SetupController } = $props();

  // Local selection seeded once from the controller; committed on Next.
  let selected = $state<string>(untrack(() => controller.profile));

  const options = [
    {
      value: "recommended",
      label: "Recommended",
      description:
        "Standard downloads, video, media processing, and archive extraction.",
    },
    {
      value: "minimal",
      label: "Minimal",
      description: "Standard downloads only.",
    },
    {
      value: "full",
      label: "Everything",
      description: "All available features, including torrent downloads.",
    },
    {
      value: "custom",
      label: "Custom",
      description: "Choose features manually on the next page.",
    },
  ];

  function next() {
    controller.applyProfile(selected as SetupProfile);
    controller.step = "features";
  }
</script>

<StageShell
  title="Choose a setup type"
  subtitle="You can change every feature later in Settings."
  onback={() => (controller.step = "welcome")}
  onnext={next}
>
  <RadioGroup legend="Setup type" {options} bind:value={selected} />
</StageShell>
