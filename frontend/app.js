const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const setupView = document.querySelector("#setup");
const assistantView = document.querySelector("#assistant");
const appHeader = document.querySelector("#app-header");
const apiKeyInput = document.querySelector("#api-key");
const modelInput = document.querySelector("#model");
const providerHint = document.querySelector("#provider-hint");
const errorBox = document.querySelector("#error");
const saveButton = document.querySelector("#save-key");
const readyMessage = document.querySelector("#ready-message");

let recognizer = null;
let transcript = "";
let isConfigured = false;
let isListening = false;
let submitWhenRecognitionEnds = false;
let requestInProgress = false;
let recognitionFailed = false;
let readyTimer = null;

function showError(message) {
  errorBox.textContent = message;
  errorBox.classList.remove("hidden");
}

function clearError() {
  errorBox.textContent = "";
  errorBox.classList.add("hidden");
}

function showSetup() {
  invoke("set_hotkey_active", { active: false }).catch((error) => showError(String(error)));
  appHeader.classList.remove("hidden");
  assistantView.classList.add("hidden");
  setupView.classList.remove("hidden");
}

async function hideWindow() {
  await invoke("hide_window");
}

async function presentWindow(compact) {
  await invoke("show_window", { compact });
}

function showAssistant() {
  appHeader.classList.add("hidden");
  setupView.classList.add("hidden");
  assistantView.classList.remove("hidden");
  clearError();
}

async function refreshProvider() {
  const key = apiKeyInput.value.trim();
  if (!key) {
    providerHint.textContent = "Provider will be detected from the key.";
    modelInput.placeholder = "Detected provider's default model";
    return;
  }

  try {
    const detected = await invoke("detect_provider", { apiKey: key });
    providerHint.textContent = `Detected: ${detected.name}. Suggested model: ${detected.default_model}`;
    if (!modelInput.value.trim()) modelInput.placeholder = detected.default_model;
  } catch (error) {
    providerHint.textContent = "This key format is not recognized.";
  }
}

apiKeyInput.addEventListener("input", () => {
  window.clearTimeout(apiKeyInput.timer);
  apiKeyInput.timer = window.setTimeout(refreshProvider, 250);
});

saveButton.addEventListener("click", async () => {
  clearError();
  saveButton.disabled = true;
  try {
    const key = apiKeyInput.value.trim();
    if (!key && isConfigured) {
      await invoke("save_model", { model: modelInput.value.trim() });
    } else {
      await invoke("save_api_key", {
        apiKey: key,
        model: modelInput.value.trim() || null,
      });
    }
    const status = await invoke("get_status");
    if (!status.configured) {
      throw new Error("Sunday could not verify its saved API key. Please save it again.");
    }
    isConfigured = true;
    apiKeyInput.value = "";
    readyMessage.classList.remove("hidden");
    setupView.querySelectorAll("label, input, .hint, #save-key").forEach((element) => {
      element.classList.add("hidden");
    });
    readyTimer = window.setTimeout(() => {
      hideWindow()
        .then(() => {
          readyMessage.classList.add("hidden");
          setupView.querySelectorAll("label, input, .hint, #save-key").forEach((element) => {
            element.classList.remove("hidden");
          });
          saveButton.disabled = false;
          readyTimer = null;
          return invoke("set_hotkey_active", { active: true });
        })
        .catch((error) => {
          showError(String(error));
          saveButton.disabled = false;
          readyTimer = null;
        });
    }, 2200);
  } catch (error) {
    saveButton.disabled = false;
    showError(String(error));
  }
});

document.querySelector("#settings-button").addEventListener("click", () => {
  clearError();
  showSetup();
});

function stopRecognition() {
  if (!recognizer) return;
  try {
    recognizer.stop();
  } catch (error) {
    showError(`Could not stop speech recognition: ${error}`);
  }
}

function startRecognition() {
  const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SpeechRecognition) {
    showError("Speech recognition is unavailable in this WebView.");
    hideWindow().catch((error) => showError(String(error)));
    return;
  }

  transcript = "";
  recognitionFailed = false;
  submitWhenRecognitionEnds = false;
  isListening = true;
  recognizer = new SpeechRecognition();
  recognizer.lang = navigator.language || "en-US";
  recognizer.interimResults = true;
  recognizer.continuous = true;
  recognizer.onresult = (event) => {
    transcript = Array.from(event.results)
      .map((result) => result[0].transcript)
      .join(" ")
      .trim();
  };
  recognizer.onerror = (event) => {
    showError(`Speech recognition failed: ${event.error}.`);
    recognitionFailed = true;
    isListening = false;
    submitWhenRecognitionEnds = false;
    stopRecognition();
  };
  recognizer.onend = () => {
    recognizer = null;
    isListening = false;
    if (submitWhenRecognitionEnds && transcript) {
      submitWhenRecognitionEnds = false;
      askQuestion(transcript);
    } else {
      submitWhenRecognitionEnds = false;
      const delay = recognitionFailed ? 2500 : 0;
      recognitionFailed = false;
      window.setTimeout(hideWindow, delay);
    }
  };

  try {
    recognizer.start();
  } catch (error) {
    recognizer = null;
    isListening = false;
    showError(`Could not start speech recognition: ${error}`);
    hideWindow().catch((hideError) => showError(String(hideError)));
  }
}

async function askQuestion(question) {
  if (!question.trim() || requestInProgress) {
    await hideWindow();
    return;
  }
  requestInProgress = true;
  let failed = false;
  try {
    const result = await invoke("ask", { question });
    await invoke("speak", { text: result.answer });
  } catch (error) {
    failed = true;
    showError(String(error));
  } finally {
    if (failed) await new Promise((resolve) => window.setTimeout(resolve, 2500));
    requestInProgress = false;
    await hideWindow().catch((error) => showError(String(error)));
  }
}

document.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && isListening) {
    event.preventDefault();
    submitWhenRecognitionEnds = true;
    isListening = false;
    stopRecognition();
  }
});

async function initialize() {
  await listen("voice-start", () => {
    if (readyTimer !== null || requestInProgress) return;
    void (async () => {
      const status = await invoke("get_status");
      isConfigured = status.configured;
      if (!isConfigured) {
        showSetup();
        await presentWindow(false);
        showError("Sunday cannot access its saved API key. Please save it again.");
        return;
      }
      showAssistant();
      await presentWindow(true);
      startRecognition();
    })().catch((error) => showError(String(error)));
  });
  const status = await invoke("get_status");
  isConfigured = status.configured;
  if (status.configured) {
    modelInput.value = status.model || "";
    providerHint.textContent = `Current provider: ${status.provider}. Leave the key blank to only change the model.`;
    saveButton.textContent = "Save settings";
    await invoke("set_hotkey_active", { active: true });
    await hideWindow();
  } else {
    showSetup();
  }
}

initialize().catch((error) => showError(String(error)));
