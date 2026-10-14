import { toast } from "sonner";
import { api } from "@/lib/api";
import { useAppStore } from "@/store/app";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

export function DeleteSound() {
  const sound = useAppStore((s) => s.deleting);
  const setDeleting = useAppStore((s) => s.setDeleting);

  return (
    <AlertDialog open={Boolean(sound)} onOpenChange={(open) => !open && setDeleting(null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove this sound?</AlertDialogTitle>
          <AlertDialogDescription>
            {sound
              ? `${sound.name} will be deleted from your local library. The original file you imported is not touched if it still lives elsewhere.`
              : ""}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={async () => {
              if (!sound) return;
              try {
                await api.removeSound(sound.id);
                toast.success("Sound removed");
              } catch (err) {
                toast.error(String(err));
              } finally {
                setDeleting(null);
              }
            }}
          >
            Remove
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
