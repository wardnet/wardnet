import { useNavigate } from "react-router";
import { PageHeader } from "@/components/compound/PageHeader";
import { PowerCard } from "@/components/features/PowerCard";
import { RestartProgressDialog } from "@/components/features/RestartProgressDialog";
import { ShutdownProgressDialog } from "@/components/features/ShutdownProgressDialog";
import { useReboot } from "@wardnet/wardnet-web";
import { useRestart } from "@wardnet/wardnet-web";
import { useShutdown } from "@wardnet/wardnet-web";
import { useAuthStore } from "@wardnet/wardnet-web";

/** Power page (admin only) — restart daemon, reboot host, shutdown
 *  host. Moved out of Settings so the menu's `System / Power` entry
 *  maps directly to a dedicated page. The three lifecycle dialogs
 *  (restart, reboot, shutdown) live here too because they're driven
 *  by the same hooks. */
export default function Power() {
  const navigate = useNavigate();
  const logout = useAuthStore((s) => s.logout);

  // Three independent hooks, one shared poll implementation under
  // the hood (see `useDaemonReachability`).
  const restart = useRestart();
  const reboot = useReboot();
  const shutdown = useShutdown();

  const anyPowerOpInFlight = restart.isOpen || reboot.isOpen || shutdown.isOpen;

  const onRestartReady = () => {
    restart.reset();
  };
  const onRestartSignIn = () => {
    // Session cookie was invalidated by the restart (e.g. in-memory
    // session store). Clear local admin flag and route to login.
    logout();
    restart.reset();
    void navigate("/login");
  };

  const onRebootReady = () => {
    reboot.reset();
  };
  const onRebootSignIn = () => {
    logout();
    reboot.reset();
    void navigate("/login");
  };

  return (
    <>
      <div className="col gap-20">
        <PageHeader
          title="Power"
          description="Restart the daemon, reboot the system, or shut Wardnet down."
        />

        <PowerCard
          onReboot={() => reboot.start()}
          onShutdown={() => shutdown.start()}
          onRestartDaemon={() => restart.start()}
          busy={anyPowerOpInFlight}
        />
      </div>

      {/* Restart-daemon flow. */}
      <RestartProgressDialog
        open={restart.isOpen}
        phase={restart.phase}
        startedAt={restart.startedAt}
        errorMessage={restart.errorMessage}
        onDismiss={onRestartReady}
        onSignIn={onRestartSignIn}
      />

      {/* Safe-Reboot flow — same lifecycle shape as restart, so the
          same dialog covers both. */}
      <RestartProgressDialog
        open={reboot.isOpen}
        phase={reboot.phase}
        startedAt={reboot.startedAt}
        errorMessage={reboot.errorMessage}
        onDismiss={onRebootReady}
        onSignIn={onRebootSignIn}
      />

      {/* Safe-Shutdown flow — distinct dialog because the terminal
          state is "host is off, no recovery" rather than "back up". */}
      <ShutdownProgressDialog
        open={shutdown.isOpen}
        phase={shutdown.phase}
        startedAt={shutdown.startedAt}
        errorMessage={shutdown.errorMessage}
        onDismiss={() => shutdown.reset()}
      />
    </>
  );
}
