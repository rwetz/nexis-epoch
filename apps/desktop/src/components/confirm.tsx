/**
 * `await confirmAction({...})` from anywhere: one AlertDialog rendered by
 * <ConfirmHost/> at the root resolves the promise.
 */
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@nexis/design";
import { create } from "zustand";

type Request = {
  title: string;
  description: string;
  confirmLabel: string;
  destructive?: boolean;
  resolve: (ok: boolean) => void;
};

const useConfirm = create<{ request: Request | null }>(() => ({ request: null }));

export function confirmAction(opts: Omit<Request, "resolve">): Promise<boolean> {
  return new Promise((resolve) => {
    useConfirm.getState().request?.resolve(false);
    useConfirm.setState({ request: { ...opts, resolve } });
  });
}

export function ConfirmHost() {
  const request = useConfirm((s) => s.request);
  const settle = (ok: boolean) => {
    request?.resolve(ok);
    useConfirm.setState({ request: null });
  };
  return (
    <AlertDialog open={request !== null} onOpenChange={(open) => !open && settle(false)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{request?.title}</AlertDialogTitle>
          <AlertDialogDescription>{request?.description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={() => settle(false)}>Cancel</AlertDialogCancel>
          <AlertDialogAction variant={request?.destructive ? "destructive" : "default"} onClick={() => settle(true)}>
            {request?.confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
