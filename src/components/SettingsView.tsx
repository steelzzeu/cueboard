import { useEffect, useState } from "react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { formatHotkey } from "@/lib/hotkey";
import type { MinimizeBehavior, RetriggerMode, Settings } from "@/lib/types";
import { useAppStore } from "@/store/app";
import { HotkeyField } from "@/components/HotkeyField";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export function SettingsView() {
  const snapshot = useAppStore((s) => s.snapshot);
  const devices = useAppStore((s) => s.devices);
  const audio = useAppStore((s) => s.audio);
  const [libraryPath, setLibraryPath] = useState("");
  const [master, setMaster] = useState(snapshot?.settings.masterVolume ?? 0.8);
  const [monitor, setMonitor] = useState(snapshot?.settings.monitorVolume ?? 1);

  useEffect(() => {
    api.libraryPath().then(setLibraryPath).catch(() => undefined);
    api.devices().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (snapshot) {
      setMaster(snapshot.settings.masterVolume);
      setMonitor(snapshot.settings.monitorVolume);
    }
  }, [snapshot?.settings.masterVolume, snapshot?.settings.monitorVolume]);

  if (!snapshot) return null;
  const settings = snapshot.settings;

  const save = async (patch: Partial<Settings>) => {
    try {
      await api.updateSettings({ ...settings, ...patch });
    } catch (err) {
      toast.error(String(err));
    }
  };

  return (
    <ScrollArea className="flex-1">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-6">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">Settings</h1>
          <p className="text-sm text-muted-foreground">
            Audio, shortcuts and window behaviour stay on this PC. Cueboard does not create an account or send your sounds anywhere.
          </p>
        </div>

        <Tabs defaultValue="audio">
          <TabsList variant="line">
            <TabsTrigger value="audio">Audio</TabsTrigger>
            <TabsTrigger value="hotkeys">Hotkeys</TabsTrigger>
            <TabsTrigger value="window">Window</TabsTrigger>
            <TabsTrigger value="playback">Playback</TabsTrigger>
            <TabsTrigger value="routing">Stream routing</TabsTrigger>
          </TabsList>

          <TabsContent value="audio" className="mt-6 grid gap-6">
            <Section title="Output device" body="Sounds play through this Windows playback device.">
              <Select
                value={settings.outputDevice ?? "default"}
                onValueChange={(value) => {
                  if (value === "default") {
                    save({ outputDevice: null, outputDeviceName: null });
                    return;
                  }
                  const device = devices.find((d) => d.id === value);
                  save({
                    outputDevice: value,
                    outputDeviceName: device?.name ?? null,
                  });
                }}
              >
                <SelectTrigger className="w-full max-w-md">
                  <SelectValue placeholder="Windows default" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">Windows default</SelectItem>
                  {devices.map((device) => (
                    <SelectItem key={device.id} value={device.id}>
                      {device.name}
                      {device.isDefault ? " (default)" : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {audio?.status.primary.activeName ? (
                <p className="text-xs text-muted-foreground">
                  Active: {audio.status.primary.activeName}
                  {audio.status.primary.sampleRate
                    ? ` · ${audio.status.primary.sampleRate} Hz · ${audio.status.primary.channels} ch`
                    : ""}
                </p>
              ) : null}
              {audio?.status.primary.error ? (
                <Alert>
                  <AlertTitle>Output issue</AlertTitle>
                  <AlertDescription>{audio.status.primary.error}</AlertDescription>
                </Alert>
              ) : null}
            </Section>

            <Section
              title="Monitor device"
              body="Optional second output, usually your headphones, so you can hear cues while the primary device is a virtual cable."
            >
              <Select
                value={settings.monitorDevice ?? "none"}
                onValueChange={(value) => {
                  if (value === "none") {
                    save({ monitorDevice: null, monitorDeviceName: null });
                    return;
                  }
                  const device = devices.find((d) => d.id === value);
                  save({
                    monitorDevice: value,
                    monitorDeviceName: device?.name ?? null,
                  });
                }}
              >
                <SelectTrigger className="w-full max-w-md">
                  <SelectValue placeholder="None" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">None</SelectItem>
                  {devices.map((device) => (
                    <SelectItem key={device.id} value={device.id}>
                      {device.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Section>

            <Section title="Master volume" body="Applies to every sound, after each sound's own level.">
              <div className="flex max-w-md items-center gap-3">
                <Slider
                  min={0}
                  max={100}
                  value={[Math.round(master * 100)]}
                  onValueChange={([v]) => setMaster((v ?? 80) / 100)}
                  onValueCommit={([v]) => save({ masterVolume: (v ?? 80) / 100 })}
                />
                <span className="w-10 text-xs tabular-nums text-muted-foreground">
                  {Math.round(master * 100)}%
                </span>
              </div>
            </Section>

            <Section title="Monitor volume" body="Level of the optional monitor path relative to master.">
              <div className="flex max-w-md items-center gap-3">
                <Slider
                  min={0}
                  max={100}
                  value={[Math.round(monitor * 100)]}
                  onValueChange={([v]) => setMonitor((v ?? 100) / 100)}
                  onValueCommit={([v]) => save({ monitorVolume: (v ?? 100) / 100 })}
                />
                <span className="w-10 text-xs tabular-nums text-muted-foreground">
                  {Math.round(monitor * 100)}%
                </span>
              </div>
            </Section>
          </TabsContent>

          <TabsContent value="hotkeys" className="mt-6 grid gap-6">
            <Alert>
              <AlertTitle>Global shortcuts</AlertTitle>
              <AlertDescription>
                Assigned keys are registered with Windows and work while another app is focused. Cueboard never records or stores unrelated keystrokes. Some exclusive-fullscreen games swallow input before Windows sees it. If a shortcut fails in a specific game, try a function key or a modifier combination.
              </AlertDescription>
            </Alert>

            <Section title="Enable / disable soundboard" body="This shortcut stays available while the board is disabled, so you can turn it back on without opening the window. In strict minimised mode it is released until you restore the window.">
              <HotkeyField
                value={settings.toggleHotkey}
                ignore="toggle"
                onChange={(toggleHotkey) => save({ toggleHotkey })}
              />
            </Section>

            <Section title="Stop all sounds" body="Stops every playing voice immediately.">
              <HotkeyField
                value={settings.stopAllHotkey}
                ignore="stopAll"
                onChange={(stopAllHotkey) => save({ stopAllHotkey })}
              />
            </Section>

            {snapshot.warnings.length > 0 ? (
              <Alert variant="destructive">
                <AlertTitle>Some shortcuts did not register</AlertTitle>
                <AlertDescription>
                  <ul className="mt-2 list-disc space-y-1 pl-4">
                    {snapshot.warnings.map((issue) => (
                      <li key={`${issue.target}-${issue.label}`}>{issue.message}</li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : null}

            <p className="text-xs text-muted-foreground">
              Current toggle: {settings.toggleHotkey ? formatHotkey(settings.toggleHotkey) : "not assigned"}
            </p>
          </TabsContent>

          <TabsContent value="window" className="mt-6 grid gap-6">
            <Section title="When the window is minimised or hidden" body="By default, sound shortcuts turn off so they cannot fire while you type or play. The enable/disable shortcut remains available unless you choose strict mode.">
              <RadioGroup
                value={settings.minimizeBehavior}
                onValueChange={(value) => save({ minimizeBehavior: value as MinimizeBehavior })}
                className="gap-3"
              >
                <Choice
                  id="min-disable"
                  value="disable"
                  title="Disable sound shortcuts"
                  body="Recommended. The toggle shortcut can turn the board back on without restoring the window."
                />
                <Choice
                  id="min-strict"
                  value="strict"
                  title="Disable everything until the window is restored"
                  body="Sound shortcuts and the toggle shortcut are released. You must show Cueboard to use it again."
                />
                <Choice
                  id="min-keep"
                  value="keepActive"
                  title="Keep the current state"
                  body="For users who want the board live in the tray or on a second display."
                />
              </RadioGroup>
            </Section>

            <ToggleRow
              title="Re-enable when the window is shown"
              body="Turn the soundboard back on when you restore Cueboard from the taskbar or tray."
              checked={settings.restoreOnShow}
              onCheckedChange={(restoreOnShow) => save({ restoreOnShow })}
            />
            <ToggleRow
              title="Close to tray"
              body="The close button hides Cueboard instead of quitting. Use Quit from the tray menu to exit."
              checked={settings.closeToTray}
              onCheckedChange={(closeToTray) => save({ closeToTray })}
            />
            <ToggleRow
              title="Start with Windows"
              body="Launch Cueboard when you sign in."
              checked={settings.startWithWindows}
              onCheckedChange={(startWithWindows) => save({ startWithWindows })}
            />
            <ToggleRow
              title="Start minimised"
              body="Open hidden in the tray. Combined with disable-on-minimise, shortcuts stay off until you show the window or press the toggle shortcut."
              checked={settings.startMinimized}
              onCheckedChange={(startMinimized) => save({ startMinimized })}
            />
            <ToggleRow
              title="Start active"
              body="Sound shortcuts are live when Cueboard launches, unless start minimised applies your minimised rule."
              checked={settings.startActive}
              onCheckedChange={(startActive) => save({ startActive })}
            />
            <ToggleRow
              title="Keep compact mode above other windows"
              body="Useful on a second monitor next to a game or OBS."
              checked={settings.compactAlwaysOnTop}
              onCheckedChange={(compactAlwaysOnTop) => save({ compactAlwaysOnTop })}
            />
          </TabsContent>

          <TabsContent value="playback" className="mt-6 grid gap-6">
            <Section title="When a sound is triggered again" body="Controls overlapping playback of the same entry.">
              <RadioGroup
                value={settings.retriggerMode}
                onValueChange={(value) => save({ retriggerMode: value as RetriggerMode })}
              >
                <Choice id="rt-overlap" value="overlap" title="Overlap" body="Each press starts another voice." />
                <Choice id="rt-restart" value="restart" title="Restart" body="Stop the current voice and start again." />
                <Choice id="rt-toggle" value="toggle" title="Toggle" body="Press to start, press again to stop." />
              </RadioGroup>
            </Section>

            <ToggleRow
              title="Stop playing sounds when the board is disabled"
              body="If off, already-playing sounds finish. New triggers stay blocked while disabled."
              checked={settings.stopOnDisable}
              onCheckedChange={(stopOnDisable) => save({ stopOnDisable })}
            />

            <Section title="Maximum overlapping voices" body="Oldest sounds are released first when the limit is reached.">
              <div className="flex max-w-xs items-center gap-3">
                <Input
                  type="number"
                  min={1}
                  max={64}
                  value={settings.maxVoices}
                  onChange={(e) => save({ maxVoices: Number(e.target.value) || 16 })}
                />
              </div>
            </Section>

            <Section title="Library" body="Sounds and settings are stored only in this folder.">
              <p className="break-all font-mono text-xs text-muted-foreground">{libraryPath}</p>
              <div className="flex flex-wrap gap-2">
                <Button variant="outline" size="sm" onClick={() => api.openLibraryFolder()}>
                  Open folder
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    const ok = await api.exportLibrary();
                    if (ok) toast.success("Library exported");
                  }}
                >
                  Export pack
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    const report = await api.importPack();
                    if (report.imported.length) {
                      toast.success(`Imported ${report.imported.length} sounds from pack`);
                    }
                    report.failed.forEach((fail) => toast.error(`${fail.name}: ${fail.message}`));
                  }}
                >
                  Import pack
                </Button>
              </div>
            </Section>
          </TabsContent>

          <TabsContent value="routing" className="mt-6 grid gap-6">
            <Alert>
              <AlertTitle>Headphones are not a livestream input</AlertTitle>
              <AlertDescription>
                Playing a sound through your headset or speakers does not put it on stream, into Discord, or into your microphone. Windows keeps playback and capture separate. Cueboard cannot inject audio into OBS Studio, TikTok LIVE Studio or Discord. It plays into the output device you choose. To share those sounds, send that output into a virtual cable or loopback device those apps can capture.
              </AlertDescription>
            </Alert>

            <Section title="Typical streamer setup">
              <ol className="list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
                <li>Install a virtual audio device such as VB-CABLE or VoiceMeeter if you want a dedicated soundboard channel.</li>
                <li>In Cueboard, set Output to that virtual device.</li>
                <li>Set Monitor to your headphones if you still want to hear the cues locally.</li>
                <li>In OBS or TikTok LIVE Studio, add that virtual device as its own audio source, separate from your microphone.</li>
              </ol>
            </Section>

            <Section title="OBS Studio">
              <ol className="list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
                <li>Open OBS and add an Audio Input Capture or Audio Output Capture for the same device Cueboard is playing into.</li>
                <li>Keep your microphone on a different source so you can duck or mute them independently.</li>
                <li>If you only need desktop audio and Cueboard is playing to your default speakers, OBS Desktop Audio will include it. That also includes game audio.</li>
              </ol>
            </Section>

            <Section title="TikTok LIVE Studio">
              <ol className="list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
                <li>Open Settings, then Audio.</li>
                <li>Add the virtual device Cueboard uses as an extra source.</li>
                <li>Leave your microphone on its own input so voice and soundboard can be mixed separately.</li>
              </ol>
            </Section>

            <Section title="Discord">
              <p className="text-sm text-muted-foreground">
                Discord captures a microphone, not a speaker. To send Cueboard into a voice channel you need a mixer such as VoiceMeeter that combines Cueboard's output with your mic, then select that combined device as the Discord input. Cueboard does not change Discord's devices for you.
              </p>
            </Section>
          </TabsContent>
        </Tabs>
      </div>
    </ScrollArea>
  );
}

function Section({
  title,
  body,
  children,
}: {
  title: string;
  body?: string;
  children?: React.ReactNode;
}) {
  return (
    <section className="grid gap-3">
      <div>
        <h2 className="text-sm font-medium">{title}</h2>
        {body ? <p className="text-sm text-muted-foreground">{body}</p> : null}
      </div>
      {children}
    </section>
  );
}

function ToggleRow({
  title,
  body,
  checked,
  onCheckedChange,
}: {
  title: string;
  body: string;
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4 rounded-md border px-3 py-3">
      <div>
        <p className="text-sm font-medium">{title}</p>
        <p className="text-xs text-muted-foreground">{body}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}

function Choice({
  id,
  value,
  title,
  body,
}: {
  id: string;
  value: string;
  title: string;
  body: string;
}) {
  return (
    <div className="flex items-start gap-3">
      <RadioGroupItem id={id} value={value} className="mt-1" />
      <Label htmlFor={id} className="grid gap-1 font-normal">
        <span className="font-medium">{title}</span>
        <span className="text-xs text-muted-foreground">{body}</span>
      </Label>
    </div>
  );
}
