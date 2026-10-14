import { useState } from "react";
import { FolderPlus, Heart, Inbox, LayoutGrid, Settings, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { useAppStore } from "@/store/app";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";

export function Sidebar() {
  const snapshot = useAppStore((s) => s.snapshot);
  const filter = useAppStore((s) => s.filter);
  const page = useAppStore((s) => s.page);
  const setFilter = useAppStore((s) => s.setFilter);
  const setPage = useAppStore((s) => s.setPage);
  const [creating, setCreating] = useState(false);
  const [folderName, setFolderName] = useState("");
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);

  const sounds = snapshot?.library.sounds ?? [];
  const folders = snapshot?.library.folders ?? [];
  const favCount = sounds.filter((s) => s.favorite).length;
  const unfiled = sounds.filter((s) => !s.folderId).length;

  const goBoard = (next: typeof filter) => {
    setFilter(next);
    setPage("board");
  };

  return (
    <aside className="flex w-52 shrink-0 flex-col border-r bg-sidebar">
      <ScrollArea className="flex-1">
        <nav className="flex flex-col gap-0.5 p-2">
          <NavButton
            active={page === "board" && filter === "all"}
            icon={<LayoutGrid />}
            label="All sounds"
            count={sounds.length}
            onClick={() => goBoard("all")}
          />
          <NavButton
            active={page === "board" && filter === "favorites"}
            icon={<Heart />}
            label="Favourites"
            count={favCount}
            onClick={() => goBoard("favorites")}
          />
          <NavButton
            active={page === "board" && filter === "unfiled"}
            icon={<Inbox />}
            label="Unfiled"
            count={unfiled}
            onClick={() => goBoard("unfiled")}
          />

          <div className="mt-3 mb-1 flex items-center justify-between px-2">
            <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
              Folders
            </p>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="New folder"
              onClick={() => {
                setFolderName("");
                setCreating(true);
              }}
            >
              <FolderPlus />
            </Button>
          </div>

          {folders.length === 0 ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">No folders yet</p>
          ) : (
            folders.map((folder) => {
              const count = sounds.filter((s) => s.folderId === folder.id).length;
              return (
                <ContextMenu key={folder.id}>
                  <ContextMenuTrigger>
                    <NavButton
                      active={page === "board" && filter === folder.id}
                      label={folder.name}
                      count={count}
                      onClick={() => goBoard(folder.id)}
                    />
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuItem onClick={() => setRenaming({ id: folder.id, name: folder.name })}>
                      Rename
                    </ContextMenuItem>
                    <ContextMenuItem
                      variant="destructive"
                      onClick={async () => {
                        await api.deleteFolder(folder.id);
                        if (filter === folder.id) setFilter("all");
                        toast.success("Folder removed. Sounds were moved to Unfiled.");
                      }}
                    >
                      <Trash2 />
                      Delete
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
              );
            })
          )}
        </nav>
      </ScrollArea>

      <Separator />
      <div className="p-2">
        <NavButton
          active={page === "settings"}
          icon={<Settings />}
          label="Settings"
          onClick={() => setPage("settings")}
        />
      </div>

      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>New folder</DialogTitle>
          </DialogHeader>
          <div className="grid gap-2">
            <Label htmlFor="folder-name">Name</Label>
            <Input
              id="folder-name"
              value={folderName}
              autoFocus
              onChange={(e) => setFolderName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submitFolder();
              }}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreating(false)}>
              Cancel
            </Button>
            <Button onClick={submitFolder}>Create</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(renaming)} onOpenChange={(open) => !open && setRenaming(null)}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Rename folder</DialogTitle>
          </DialogHeader>
          <Input
            value={renaming?.name ?? ""}
            autoFocus
            onChange={(e) => setRenaming((cur) => (cur ? { ...cur, name: e.target.value } : cur))}
            onKeyDown={(e) => {
              if (e.key === "Enter") submitRename();
            }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenaming(null)}>
              Cancel
            </Button>
            <Button onClick={submitRename}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </aside>
  );

  async function submitFolder() {
    try {
      const folder = await api.createFolder(folderName);
      setCreating(false);
      goBoard(folder.id);
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function submitRename() {
    if (!renaming) return;
    try {
      await api.renameFolder(renaming.id, renaming.name);
      setRenaming(null);
    } catch (err) {
      toast.error(String(err));
    }
  }
}

function NavButton({
  active,
  icon,
  label,
  count,
  onClick,
}: {
  active: boolean;
  icon?: React.ReactNode;
  label: string;
  count?: number;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] transition-colors ${
        active
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "text-sidebar-foreground/80 hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground"
      }`}
    >
      {icon ? <span className="[&_svg]:size-3.5">{icon}</span> : null}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {typeof count === "number" ? (
        <span className="text-[11px] text-muted-foreground tabular-nums">{count}</span>
      ) : null}
    </button>
  );
}
