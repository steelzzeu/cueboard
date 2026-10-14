import { FileAudio, Keyboard, MonitorSpeaker } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/store/app";
import { YoutubeMark } from "@/components/YoutubeMark";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";

export function Onboarding() {
  return (
    <Empty className="h-full border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <FileAudio />
        </EmptyMedia>
        <EmptyTitle>Add your first sounds</EmptyTitle>
        <EmptyDescription>
          Drop audio files here or use the file picker. Cueboard keeps everything in a local library on this computer. Nothing is uploaded.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <div className="flex flex-wrap justify-center gap-2">
          <Button onClick={() => api.pickImport()}>Import audio files</Button>
          <Button variant="outline" onClick={() => useAppStore.getState().openYoutube()}>
            <YoutubeMark />
            Paste a YouTube link
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          MP3, WAV, OGG, FLAC, AAC, M4A, WMA and other common formats. YouTube links convert to MP3 on this PC.
        </p>
        <div className="mt-4 grid w-full gap-3 text-left text-sm">
          <Hint
            icon={<Keyboard />}
            title="Assign a keybind"
            body="Open a sound and record a shortcut. It works while games and browsers are focused."
          />
          <Hint
            icon={<MonitorSpeaker />}
            title="Headphones are not the stream"
            body="Playback through your headset stays local. Route Cueboard to a virtual cable if you want OBS or TikTok LIVE Studio to hear it."
          />
        </div>
      </EmptyContent>
    </Empty>
  );
}

function Hint({
  icon,
  title,
  body,
}: {
  icon: React.ReactNode;
  title: string;
  body: string;
}) {
  return (
    <div className="flex gap-3 rounded-md border bg-card px-3 py-2.5">
      <div className="mt-0.5 text-muted-foreground [&_svg]:size-4">{icon}</div>
      <div>
        <p className="font-medium">{title}</p>
        <p className="text-xs text-muted-foreground">{body}</p>
      </div>
    </div>
  );
}
