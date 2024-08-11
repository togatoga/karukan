console.log('app.js loaded - version 2 with token visualization');

// API endpoint
const API_BASE = 'http://localhost:3000/api';

// DOM elements
const romajiInput = document.getElementById('romaji-input');
const hiraganaOutput = document.getElementById('hiragana-output');
const bufferDisplay = document.getElementById('buffer-display');
const directHiraganaInput = document.getElementById('direct-hiragana-input');
const kanjiCandidates = document.getElementById('kanji-candidates');
const inferenceTime = document.getElementById('inference-time');
const clearBtn = document.getElementById('clear-btn');
const tokenDisplay = document.getElementById('token-display');
const tokenCount = document.getElementById('token-count');

// Mode toggle
const romajiModeBtn = document.getElementById('romaji-mode-btn');
const hiraganaModeBtn = document.getElementById('hiragana-mode-btn');
const romajiSection = document.getElementById('romaji-section');
const hiraganaSection = document.getElementById('hiragana-section');

// Model selection and parameters
const modelSelect = document.getElementById('model-select');
const modelStatus = document.getElementById('model-status');
const numCandidatesInput = document.getElementById('num-candidates-input');
const contextInput = document.getElementById('context-input');
const beamSearchTypeSelect = document.getElementById('beam-search-type');

// Character count displays
const romajiCharCount = document.getElementById('romaji-char-count');
const hiraganaCharCount = document.getElementById('hiragana-char-count');
const directHiraganaCharCount = document.getElementById('direct-hiragana-char-count');
const kanjiCharCount = document.getElementById('kanji-char-count');

// State
let debounceTimer = null;
let currentMode = 'romaji'; // 'romaji' or 'hiragana'
let currentModel = null;
let availableModels = [];

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    // Setup event listeners first (so UI works even if API fails)
    setupEventListeners();
    romajiInput.focus();
    // Then load models
    await loadModels();
});

async function loadModels() {
    try {
        const response = await fetch(`${API_BASE}/models`);
        if (!response.ok) throw new Error('Failed to load models');

        const data = await response.json();
        availableModels = data.models;
        currentModel = data.default;

        // Populate select with proper display names
        modelSelect.innerHTML = availableModels.map(model =>
            `<option value="${model.id}" ${model.id === currentModel ? 'selected' : ''}>
                ${model.name}
            </option>`
        ).join('');

        updateModelStatus();
    } catch (error) {
        console.error('Error loading models:', error);
        modelSelect.innerHTML = '<option value="">Error loading models</option>';
    }
}

function updateModelStatus(loading = false) {
    if (loading) {
        modelStatus.textContent = 'Loading...';
        modelStatus.className = 'model-status loading';
    } else {
        const model = availableModels.find(m => m.id === currentModel);
        if (model) {
            modelStatus.textContent = '';
            modelStatus.className = 'model-status';
        }
    }
}

function setupEventListeners() {
    console.log('setupEventListeners called');

    // Romaji input
    romajiInput.addEventListener('input', handleRomajiInput);

    // Direct hiragana input
    directHiraganaInput.addEventListener('input', handleDirectHiraganaInput);

    // Mode toggle
    romajiModeBtn.addEventListener('click', () => switchMode('romaji'));
    hiraganaModeBtn.addEventListener('click', () => switchMode('hiragana'));

    // Model selection
    modelSelect.addEventListener('change', handleModelChange);

    // Parameter changes
    numCandidatesInput.addEventListener('change', handleParamChange);
    contextInput.addEventListener('input', handleParamChange);
    if (beamSearchTypeSelect) {
        beamSearchTypeSelect.addEventListener('change', handleParamChange);
    }

    // Clear button
    clearBtn.addEventListener('click', clearAll);

    // Example buttons (query here to ensure DOM is ready)
    const exampleBtns = document.querySelectorAll('.example-btn');
    const exampleCtxBtns = document.querySelectorAll('.example-btn-ctx');

    console.log('Found example buttons:', exampleBtns.length);
    console.log('Found context buttons:', exampleCtxBtns.length);

    exampleBtns.forEach(btn => {
        btn.addEventListener('click', async () => {
            const text = btn.getAttribute('data-text');
            // Clear first, then set values
            await clearAll();
            // Make sure we're in romaji mode
            if (currentMode !== 'romaji') {
                currentMode = 'romaji';
                romajiModeBtn.classList.add('active');
                hiraganaModeBtn.classList.remove('active');
                romajiSection.classList.remove('hidden');
                hiraganaSection.classList.add('hidden');
            }
            romajiInput.value = text;
            handleRomajiInput({ target: romajiInput });
        });
    });

    // Context-aware example buttons (use hiragana directly)
    exampleCtxBtns.forEach((btn, idx) => {
        console.log(`Attaching listener to context button ${idx}:`, btn.getAttribute('data-text'));
        btn.addEventListener('click', async () => {
            console.log('Context button clicked!');
            const hiragana = btn.getAttribute('data-text');
            const context = btn.getAttribute('data-context');
            console.log(`Hiragana: ${hiragana}, Context: ${context}`);
            // Set context
            if (contextInput) {
                contextInput.value = context;
                console.log('Context set to:', contextInput.value);
            } else {
                console.error('contextInput element not found!');
            }
            // Switch to hiragana mode and set input
            if (currentMode !== 'hiragana') {
                currentMode = 'hiragana';
                romajiModeBtn.classList.remove('active');
                hiraganaModeBtn.classList.add('active');
                romajiSection.classList.add('hidden');
                hiraganaSection.classList.remove('hidden');
            }
            directHiraganaInput.value = hiragana;
            console.log('Calling convertToKanji with:', hiragana);
            // Directly convert to kanji
            await convertToKanji(hiragana);
        });
    });
}

async function handleModelChange(e) {
    const newModel = e.target.value;
    if (newModel === currentModel) return;

    currentModel = newModel;
    updateModelStatus(true);

    // Trigger re-conversion with new model if there's input
    if (currentMode === 'romaji' && romajiInput.value) {
        await handleRomajiInput({ target: romajiInput });
    } else if (currentMode === 'hiragana' && directHiraganaInput.value) {
        await handleDirectHiraganaInput({ target: directHiraganaInput });
    }

    updateModelStatus(false);
}

async function handleParamChange() {
    // Trigger re-conversion with new parameters if there's input
    if (currentMode === 'romaji' && romajiInput.value) {
        await handleRomajiInput({ target: romajiInput });
    } else if (currentMode === 'hiragana' && directHiraganaInput.value) {
        await handleDirectHiraganaInput({ target: directHiraganaInput });
    }
}

function switchMode(mode) {
    currentMode = mode;

    if (mode === 'romaji') {
        romajiModeBtn.classList.add('active');
        hiraganaModeBtn.classList.remove('active');
        romajiSection.classList.remove('hidden');
        hiraganaSection.classList.add('hidden');
        romajiInput.focus();
    } else {
        romajiModeBtn.classList.remove('active');
        hiraganaModeBtn.classList.add('active');
        romajiSection.classList.add('hidden');
        hiraganaSection.classList.remove('hidden');
        directHiraganaInput.focus();
    }

    clearAll();
}

async function handleRomajiInput(e) {
    const input = e.target.value;

    // Update romaji character count
    updateCharCount(romajiCharCount, input.length);

    if (!input) {
        hiraganaOutput.textContent = '';
        bufferDisplay.textContent = '';
        kanjiCandidates.innerHTML = '<p class="placeholder-text">Type to see kanji candidates</p>';
        inferenceTime.textContent = '';
        updateCharCount(hiraganaCharCount, 0);
        updateCharCount(kanjiCharCount, 0);
        return;
    }

    try {
        // Step 1: Romaji -> Hiragana
        const romajiResponse = await fetch(`${API_BASE}/convert`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ input, incremental: false }),
        });

        if (!romajiResponse.ok) throw new Error('Romaji conversion failed');

        const romajiData = await romajiResponse.json();
        hiraganaOutput.textContent = romajiData.output;
        bufferDisplay.textContent = romajiData.buffer;

        // Get full hiragana (including buffer)
        const hiragana = romajiData.output + romajiData.buffer;

        // Update hiragana character count
        updateCharCount(hiraganaCharCount, hiragana.length);

        if (!hiragana) {
            kanjiCandidates.innerHTML = '<p class="placeholder-text">Type to see kanji candidates</p>';
            inferenceTime.textContent = '';
            return;
        }

        // Step 2: Hiragana -> Kanji (debounced)
        clearTimeout(debounceTimer);
        debounceTimer = setTimeout(() => convertToKanji(hiragana), 300);

    } catch (error) {
        console.error('Error:', error);
        hiraganaOutput.textContent = 'Error';
        bufferDisplay.textContent = '';
    }
}

async function handleDirectHiraganaInput(e) {
    const hiragana = e.target.value;

    // Update direct hiragana character count
    updateCharCount(directHiraganaCharCount, hiragana.length);

    if (!hiragana) {
        kanjiCandidates.innerHTML = '<p class="placeholder-text">Type to see kanji candidates</p>';
        inferenceTime.textContent = '';
        updateCharCount(kanjiCharCount, 0);
        return;
    }

    // Debounced conversion
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => convertToKanji(hiragana), 300);
}

async function convertToKanji(hiragana) {
    kanjiCandidates.innerHTML = '<p class="loading-text">Converting...</p>';
    inferenceTime.textContent = '';

    // Get num_candidates value from input (1-10 range)
    const numCandidates = parseInt(numCandidatesInput.value, 10);
    const validNumCandidates = (numCandidates && numCandidates >= 1 && numCandidates <= 10) ? numCandidates : 1;

    try {
        const context = contextInput ? contextInput.value : '';
        const beamSearchType = beamSearchTypeSelect ? beamSearchTypeSelect.value : 'true';
        const requestBody = {
            hiragana,
            context: context,
            num_candidates: validNumCandidates,
            model: currentModel,
            beam_search_type: beamSearchType,
        };

        console.log('Sending request:', JSON.stringify(requestBody));

        const response = await fetch(`${API_BASE}/kanji/convert`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
        });

        if (!response.ok) {
            const errorText = await response.text();
            kanjiCandidates.innerHTML = `<p class="error-text">Error: ${errorText}</p>`;
            return;
        }

        const data = await response.json();
        console.log('API response:', JSON.stringify(data, null, 2));
        displayKanjiCandidates(data.candidates);

        // Display inference time and model
        if (data.inference_time_ms !== undefined) {
            inferenceTime.textContent = `${data.inference_time_ms.toFixed(1)} ms (${data.model}, n=${data.candidates.length})`;
        }

        // Display token visualization for all candidates
        console.log('Candidate tokens:', data.candidate_tokens);
        if (data.candidate_tokens && data.candidate_tokens.length > 0) {
            displayCandidateTokens(data.candidate_tokens, data.beam_search_type);
        } else if (data.tokens && data.tokens.length > 0) {
            // Fallback to legacy format
            displayTokens(data.tokens, data.input_tokens, data.output_tokens);
        } else {
            console.log('No tokens, showing placeholder');
            tokenDisplay.innerHTML = '<p class="placeholder-text">Token data not available</p>';
            tokenCount.textContent = '';
        }
    } catch (error) {
        console.error('Kanji conversion error:', error);
        kanjiCandidates.innerHTML = '<p class="error-text">Kanji conversion unavailable</p>';
    }
}

// Display tokens for all candidates with scores
function displayCandidateTokens(candidateTokens, beamSearchType) {
    console.log('displayCandidateTokens called with', candidateTokens.length, 'candidates');
    if (!candidateTokens || candidateTokens.length === 0) {
        tokenDisplay.innerHTML = '<p class="placeholder-text">No token data</p>';
        tokenCount.textContent = '';
        return;
    }

    // Update header with beam search type
    const bsType = beamSearchType || 'unknown';
    tokenCount.textContent = `${candidateTokens.length} candidates (${bsType} beam search)`;

    const html = candidateTokens.map((candidate, idx) => {
        const processedTokens = groupByteTokens(candidate.tokens);
        const tokensHtml = renderTokens(processedTokens);
        const scoreDisplay = candidate.score !== 0 ? `score: ${candidate.score.toFixed(3)}` : '';
        const inputCount = candidate.input_token_count || 0;
        const outputCount = candidate.output_token_count || 0;
        const tokenCountDisplay = `in: ${inputCount}, out: ${outputCount}`;

        return `
            <div class="candidate-token-group">
                <div class="candidate-token-header">
                    <span class="candidate-token-index">#${idx + 1}</span>
                    <span class="candidate-token-text">${escapeHtml(candidate.text)}</span>
                    <span class="candidate-token-meta">${tokenCountDisplay}${scoreDisplay ? ' | ' + scoreDisplay : ''}</span>
                </div>
                <div class="candidate-token-list">${tokensHtml}</div>
            </div>
        `;
    }).join('');

    tokenDisplay.innerHTML = html;
}

// Special token characters (PUA - Private Use Area)
// These are the same across all model families; only the token IDs differ.
const INPUT_START_CHAR = '\uEE00';
const OUTPUT_START_CHAR = '\uEE01';
const CONTEXT_CHAR = '\uEE02';

function getSpecialTokenLabel(token) {
    if (token.text === INPUT_START_CHAR) return '⟨IN⟩';
    if (token.text === OUTPUT_START_CHAR) return '⟨OUT⟩';
    if (token.text === CONTEXT_CHAR) return '⟨CTX⟩';
    return null;
}

// Render tokens to HTML (shared between displayTokens and displayCandidateTokens)
function renderTokens(processedTokens) {
    return processedTokens.map(token => {
        const specialLabel = getSpecialTokenLabel(token);
        const isSpecial = specialLabel !== null;
        const isWhitespace = !isSpecial && !token.isByteGroup && (token.text.trim() === '' || token.text === '\n');

        let tokenClass = token.token_type || 'output';
        if (isSpecial) tokenClass = 'special';
        if (isWhitespace) tokenClass = 'whitespace';
        if (token.isByteGroup) tokenClass += ' byte-group';

        let displayText = specialLabel || token.text;
        if (!isSpecial) {
            if (token.text === '\n') displayText = '↵';
            else if (token.text === ' ') displayText = '·';
        }

        const tooltip = token.isByteGroup
            ? `${token.byteCount} bytes: ${token.hexDisplay}`
            : `ID: ${token.id}`;

        return `<span class="token ${tokenClass}" data-id="${token.id}" title="${tooltip}">${escapeHtml(displayText)}</span>`;
    }).join('');
}

function displayTokens(tokens, inputCount, outputCount) {
    console.log('displayTokens called with', tokens.length, 'tokens');
    if (!tokens || tokens.length === 0) {
        tokenDisplay.innerHTML = '<p class="placeholder-text">No tokens</p>';
        tokenCount.textContent = '';
        return;
    }

    // Update token count display
    tokenCount.textContent = `Input: ${inputCount || 0} tokens, Output: ${outputCount || 0} tokens`;
    console.log('tokenCount set to:', tokenCount.textContent);

    // Group and process tokens (combining byte tokens into characters)
    const processedTokens = groupByteTokens(tokens);

    const html = renderTokens(processedTokens);

    console.log('Generated token HTML:', html);
    tokenDisplay.innerHTML = html;
    console.log('tokenDisplay.innerHTML set, length:', tokenDisplay.innerHTML.length);
}

// Check if token text is a hex byte like <EF>
function isHexByteToken(text) {
    return /^<[0-9A-Fa-f]{2}>$/.test(text);
}

// Extract byte value from hex token like <EF> -> 0xEF
function extractByteValue(text) {
    const match = text.match(/^<([0-9A-Fa-f]{2})>$/);
    return match ? parseInt(match[1], 16) : null;
}

// Try to decode bytes as UTF-8
function tryDecodeUtf8(bytes) {
    try {
        const uint8 = new Uint8Array(bytes);
        const decoder = new TextDecoder('utf-8', { fatal: true });
        return decoder.decode(uint8);
    } catch {
        return null;
    }
}

// Group consecutive hex byte tokens and try to decode as UTF-8
function groupByteTokens(tokens) {
    const result = [];
    let i = 0;

    while (i < tokens.length) {
        const token = tokens[i];

        // Check if this is a hex byte token
        if (isHexByteToken(token.text)) {
            // Collect consecutive hex byte tokens of the same type (input/output)
            const byteTokens = [token];
            const tokenType = token.token_type;
            let j = i + 1;

            while (j < tokens.length &&
                   isHexByteToken(tokens[j].text) &&
                   tokens[j].token_type === tokenType) {
                byteTokens.push(tokens[j]);
                j++;
            }

            // Try to decode collected bytes as UTF-8
            const bytes = byteTokens.map(t => extractByteValue(t.text));
            const decoded = tryDecodeUtf8(bytes);
            const hexDisplay = byteTokens.map(t => t.text).join('');

            if (decoded && decoded.length > 0) {
                // Successfully decoded - show the character
                result.push({
                    id: byteTokens[0].id,
                    text: decoded,
                    token_type: tokenType,
                    isByteGroup: true,
                    byteCount: bytes.length,
                    hexDisplay: hexDisplay
                });
            } else {
                // Couldn't decode - show hex bytes grouped
                result.push({
                    id: byteTokens[0].id,
                    text: hexDisplay,
                    token_type: tokenType,
                    isByteGroup: true,
                    byteCount: bytes.length,
                    hexDisplay: hexDisplay
                });
            }

            i = j;
        } else {
            // Regular token - pass through
            result.push({
                ...token,
                isByteGroup: false
            });
            i++;
        }
    }

    return result;
}

function displayKanjiCandidates(candidates) {
    if (!candidates || candidates.length === 0) {
        kanjiCandidates.innerHTML = '<p class="placeholder-text">No candidates found</p>';
        updateCharCount(kanjiCharCount, 0);
        return;
    }

    // Update kanji character count (first candidate)
    updateCharCount(kanjiCharCount, candidates[0].length);

    const html = candidates.map((candidate, index) => `
        <div class="candidate-item" data-text="${escapeHtml(candidate)}">
            <span class="candidate-number">${index + 1}</span>
            <span class="candidate-text">${escapeHtml(candidate)}</span>
        </div>
    `).join('');

    kanjiCandidates.innerHTML = html;

    // Click to copy
    document.querySelectorAll('.candidate-item').forEach(item => {
        item.addEventListener('click', () => {
            const text = item.getAttribute('data-text');
            copyToClipboard(text);
            showCopyFeedback(item);
        });
    });
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Update character count display
function updateCharCount(element, count) {
    if (element) {
        element.textContent = count > 0 ? `(${count}chars)` : '';
    }
}

async function copyToClipboard(text) {
    try {
        await navigator.clipboard.writeText(text);
    } catch (error) {
        console.error('Failed to copy:', error);
    }
}

function showCopyFeedback(element) {
    element.classList.add('copied');
    setTimeout(() => element.classList.remove('copied'), 500);
}

async function clearAll() {
    romajiInput.value = '';
    directHiraganaInput.value = '';
    if (contextInput) contextInput.value = '';
    hiraganaOutput.textContent = '';
    bufferDisplay.textContent = '';
    kanjiCandidates.innerHTML = '<p class="placeholder-text">Type to see kanji candidates</p>';
    inferenceTime.textContent = '';
    if (tokenDisplay) tokenDisplay.innerHTML = '<p class="placeholder-text">Tokens will appear here</p>';
    if (tokenCount) tokenCount.textContent = '';

    // Clear character counts
    updateCharCount(romajiCharCount, 0);
    updateCharCount(hiraganaCharCount, 0);
    updateCharCount(directHiraganaCharCount, 0);
    updateCharCount(kanjiCharCount, 0);

    try {
        await fetch(`${API_BASE}/reset`, { method: 'POST' });
    } catch (error) {
        console.error('Error resetting:', error);
    }

    if (currentMode === 'romaji') {
        romajiInput.focus();
    } else {
        directHiraganaInput.focus();
    }
}
