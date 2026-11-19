package app.openbus

import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge

/**
 * The library: Tauri's activity, the web page's. Back, as a TV remote has
 * it, is the page's, as a gamepad's B is: out of a game's settings, onto the
 * shelf of games, the search cleared; and only with nowhere to go back
 * from, Android's, out of the app. The page has the remote's keys from the
 * first: the webview focused as it's made, and as the app comes back, not
 * by that first key, which Android would take for it. A gamepad's buttons
 * are the page's alone, as the webview's gamepads: not made Android's OK
 * and Back too, as it makes a gamepad's A and B that nothing's taken.
 */
class MainActivity : TauriActivity() {
  override val handleBackNavigation: Boolean = false
  private var page: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun dispatchKeyEvent(event: KeyEvent): Boolean {
    val from = { source: Int -> event.source and source == source }
    val pad = from(InputDevice.SOURCE_GAMEPAD) || from(InputDevice.SOURCE_JOYSTICK)
    if (pad && KeyEvent.isGamepadButton(event.keyCode)) {
      super.dispatchKeyEvent(event)
      return true
    }
    return super.dispatchKeyEvent(event)
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) {
      page?.requestFocus()
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    page = webView
    webView.requestFocus()
    val back = object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        webView.evaluateJavascript("window.openBusBack?.() === true") { went ->
          if (went != "true") {
            isEnabled = false
            onBackPressedDispatcher.onBackPressed()
            isEnabled = true
          }
        }
      }
    }
    onBackPressedDispatcher.addCallback(this, back)
  }
}
