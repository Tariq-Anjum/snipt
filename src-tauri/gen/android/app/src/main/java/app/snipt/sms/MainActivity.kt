package app.snipt.sms

import android.os.Bundle
import android.os.Process
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  /**
   * Leaving the app (back / swipe away): tao ends its event loop with
   * std::process::exit, whose libc teardown races the still-running tokio
   * workers (FORTIFY "pthread_mutex_lock called on a destroyed mutex", seen
   * on the arm64 phone on every exit). End the process before that teardown.
   * SQLite (WAL) is crash-safe, and a receiver caught mid-ingest is
   * reconciled from the system SMS provider on the next start. Not on a
   * configuration change (rotation), where the activity is recreated.
   */
  override fun onDestroy() {
    if (isFinishing && !isChangingConfigurations) {
      Process.killProcess(Process.myPid())
    }
    super.onDestroy()
  }
}
