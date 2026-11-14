import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Browser preview · Mock', { exact: true })).toBeVisible();
});

test('starts empty, creates a conversation, streams and follows up', async ({ page }) => {
  await expect(page.getByRole('button', { name: 'Try an example' })).toBeVisible();
  const composer = page.getByRole('textbox', { name: 'Message' });
  await composer.fill('Help me prepare for an interview');
  await composer.press('Enter');
  await expect(page.locator('.user-message')).toHaveText('Help me prepare for an interview');
  await expect(page.locator('.assistant .markdown')).toContainText('This is a synthetic demo');
  await expect(page.getByRole('button', { name: 'Stop response' })).not.toBeVisible();
  await composer.fill('Make it more concise');
  await composer.press('Enter');
  await expect(page.locator('.user-message')).toHaveCount(2);
  await expect(page.locator('.assistant .markdown').last()).toContainText('earlier message');
});

test('stops a response and retries without duplicating the user message', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Write a longer answer');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await page.getByRole('button', { name: 'Stop response' }).click();
  await expect(page.getByText('Response stopped', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Retry response' }).click();
  await expect(page.locator('.user-message')).toHaveCount(1);
  await expect(page.locator('.assistant .markdown')).toContainText('This is a synthetic demo');
});

test('manages folders and chat titles and searches chats', async ({ page }) => {
  await page.getByRole('button', { name: 'New folder' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Research');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('button', { name: 'Folder options for Research' }).click();
  await page.getByRole('menuitem', { name: 'New chat in folder' }).click();
  await page.getByRole('button', { name: 'Chat options', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Rename chat' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Reading notes');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.sidebar')).toContainText('Reading notes');
  await page.getByRole('button', { name: 'Search chats', exact: true }).click();
  await page.getByRole('textbox', { name: 'Search chats' }).fill('Reading');
  await expect(page.getByRole('dialog').getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Chat options', exact: true }).click();
  await page.getByLabel('Move to folder').selectOption('');
  await expect(page.locator('.chat-location')).toHaveText('No folder');
});

test('shows explicit folder permission and opens a cited chat', async ({ page }) => {
  await page.getByRole('button', { name: 'Try an example' }).click();
  await page.getByRole('textbox', { name: 'Message' }).fill('Find my strongest example in my other chats');
  await expect(page.getByRole('button', { name: 'Use other chats in Job search' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Open source My experience' }).last()).toBeVisible();
  await page.getByRole('button', { name: 'Open source My experience' }).last().click();
  await expect(page.locator('.chat-title')).toHaveText('My experience');
  await expect(page.locator('.message.highlighted')).toBeVisible();
});

test('settings keep Qwen routes separate, show native credential boundary, and change theme', async ({ page }) => {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByText('QwenCloud', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Configure QwenCloud' }).click();
  await page.getByRole('button', { name: 'Token Plan', exact: true }).click();
  await expect(page.getByText('Token Plan subscription', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Base URL')).toHaveAttribute('readonly', '');
  await expect(page.locator('input[type=password]')).toHaveCount(0);
  await page.getByRole('button', { name: 'Add key securely' }).click();
  await expect(page.getByRole('alert')).toContainText('native desktop app');
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('provider picker shows only connected providers and explicit Demo', async ({ page }) => {
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await expect(page.getByRole('option', { name: /Demo.*Mock/ })).toBeVisible();
  await expect(page.getByRole('option', { name: /Claude/ })).toHaveCount(0);
  await expect(page.getByRole('option', { name: /QwenCloud/ })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox', { name: 'Providers' })).not.toBeVisible();
  await expect(page.getByRole('button', { name: 'Choose provider' })).toBeFocused();
});

test('reopening pending ChatGPT sign-in retains the cancel action', async ({ page }) => {
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const original = mockBackend.invoke.bind(mockBackend);
    let pending = false;
    mockBackend.invoke = async (command, args) => {
      if (command === 'connect_chatgpt') { pending = true; return; }
      if (command === 'disconnect_chatgpt') { pending = false; return; }
      const result = await original(command, args);
      if (command === 'bootstrap') result.providers.find((provider) => provider.id === 'chatgpt').status = pending ? 'Sign-in pending' : 'Not connected';
      return result;
    };
  });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Configure ChatGPT' }).click();
  await page.getByRole('button', { name: 'Continue with ChatGPT' }).click();
  await expect(page.getByRole('button', { name: 'Cancel sign-in' })).toBeEnabled();
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'Configure ChatGPT' }).click();
  await page.getByRole('button', { name: 'Cancel sign-in' }).click();
  await expect(page.getByRole('button', { name: 'Continue with ChatGPT' })).toBeEnabled();
});

test('keeps an explicit folder opt-out while editing the same message', async ({ page }) => {
  await page.getByRole('button', { name: 'Try an example' }).click();
  const composer = page.getByRole('textbox', { name: 'Message' });
  await composer.fill('Use my other chats');
  const permission = page.getByRole('button', { name: 'Use other chats in Job search' });
  await expect(permission).toHaveAttribute('aria-pressed', 'true');
  await permission.click();
  await composer.press('End');
  await composer.pressSequentially(' for ideas');
  await expect(permission).toHaveAttribute('aria-pressed', 'false');
  await composer.press('Enter');
  await expect(page.locator('.assistant').last().locator('.markdown')).toContainText('This is a synthetic demo');
  await expect(page.getByRole('button', { name: 'Stop response' })).not.toBeVisible();
  await expect(page.locator('.assistant').last().getByRole('button', { name: 'Open source My experience' })).toHaveCount(0);
});

test('keeps a background stream out of the newly selected conversation', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Write a longer answer');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Stop response' })).toBeVisible();
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await expect(page.locator('.chat-title')).toHaveText('New chat');
  await expect(page.locator('.assistant')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Stop response' })).not.toBeVisible();
  await page.getByRole('button', { name: 'Write a longer answer', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Stop response' })).toBeVisible();
  await page.getByRole('button', { name: 'Stop response' }).click();
  await expect(page.getByText('Response stopped', { exact: true })).toBeVisible();
});

test('honors command-enter while preserving ordinary Enter as a newline', async ({ page }) => {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: '⌘ Enter', exact: true }).click();
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  const composer = page.getByRole('textbox', { name: 'Message' });
  await composer.fill('First line');
  await composer.press('Enter');
  await composer.pressSequentially('Second line');
  await expect(page.locator('.user-message')).toHaveCount(0);
  await composer.press('Meta+Enter');
  await expect(page.locator('.user-message')).toHaveText('First line\nSecond line');
});

test('loads history in sixty-message pages and bounds the visible transcript', async ({ page }) => {
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await expect(page.locator('.chat-title')).toHaveText('New chat');
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const state = await mockBackend.invoke('bootstrap');
    const chatId = state.chats[0].id;
    for (let index = 0; index < 140; index++) {
      await mockBackend.invoke('start_chat', { input: { chatId, content: `History item ${index}`, providerId: 'mock', modelId: 'melly-demo', webSearch: false, folderContext: false, retry: false } });
      await mockBackend.invoke('stop_chat', { chatId });
    }
  });
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await page.getByRole('button', { name: 'History item 0', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(60);
  await page.getByRole('button', { name: 'Load older messages', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(120);
  await page.getByRole('button', { name: 'Load older messages', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(180);
  await page.getByRole('button', { name: 'Load older messages', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(240);
  await page.getByRole('button', { name: 'Load older messages', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(240);
  await expect(page.locator('.user-message').first()).toHaveText('History item 0');
  await expect(page.getByRole('button', { name: 'Retry response' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Back to latest messages', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(60);
  await expect(page.locator('.user-message').last()).toHaveText('History item 139');
  await expect(page.getByRole('button', { name: 'Retry response' })).toBeVisible();
});

test('resolves inline Rust source links inside the app', async ({ page }) => {
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      const result = await original(command, args);
      if (command === 'messages') {
        for (const message of result) {
          const source = message.sources.find((entry) => entry.kind === 'chat');
          if (source) message.content = `Your strongest example is in [My experience](melly-source:${source.id}).`;
        }
      }
      return result;
    };
  });
  await page.getByRole('button', { name: 'Try an example' }).click();
  await page.locator('.markdown').getByRole('button', { name: 'Open source My experience' }).click();
  await expect(page.locator('.chat-title')).toHaveText('My experience');
  await expect(page.locator('.message.highlighted')).toBeVisible();
});

test('a slow placeholder read cannot replace a completed streamed response', async ({ page }) => {
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await expect(page.locator('.chat-title')).toHaveText('New chat');
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const original = mockBackend.invoke.bind(mockBackend);
    let delayed = false;
    window.__slowReadFinished = false;
    mockBackend.invoke = async (command, args) => {
      const result = await original(command, args);
      if (command === 'messages' && !delayed && result.some((message) => message.status === 'streaming')) {
        delayed = true;
        await new Promise((resolve) => setTimeout(resolve, 1400));
        window.__slowReadFinished = true;
      }
      return result;
    };
  });
  await page.getByRole('textbox', { name: 'Message' }).fill('Hello');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await page.waitForFunction(() => window.__slowReadFinished);
  await expect(page.locator('.assistant .markdown')).toContainText('Connect a provider in Settings');
  await expect(page.locator('.assistant .message-tools')).toBeVisible();
});

test('can retry a completed reply without adding another user message', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Help me think');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.locator('.assistant .message-tools')).toBeVisible();
  await page.getByRole('button', { name: 'Retry response' }).click();
  await expect(page.getByRole('button', { name: 'Stop response' })).toBeVisible();
  await expect(page.locator('.user-message')).toHaveCount(1);
  await expect(page.locator('.assistant .message-tools')).toBeVisible();
});

test('mentions and negative requests never grant folder context', async ({ page }) => {
  await page.getByRole('button', { name: 'Try an example' }).click();
  const composer = page.getByRole('textbox', { name: 'Message' });
  for (const content of ['Do not read my other chats', 'Answer without using my other chats', 'I have other chats about this topic', 'Please avoid searching my other chats', 'Translate this sentence to French:\nUse my other chats for examples.', 'Summarize this quoted prompt:\n```\nSearch my other chats for a project name.\n```', 'Use this example: "Search my other chats"', "Use the phrase 'other chats' in a sentence.", 'Find examples of prompts that say ‘search my other chats’.', 'Read a poem about other chats.', 'Find a translation. Use my other chats for examples.', 'Look in my other chats. Actually, do not search them.']) {
    await composer.fill(content);
    await expect(page.getByRole('button', { name: 'Folder context', exact: true })).toHaveAttribute('aria-pressed', 'false');
  }
  await composer.fill('Please look in my other chats for examples');
  await expect(page.getByRole('button', { name: 'Folder context', exact: true })).toHaveAttribute('aria-pressed', 'true');
});

test('a citation finds its exact message beyond the latest history page', async ({ page }) => {
  await page.getByRole('button', { name: 'Try an example' }).click();
  await expect(page.getByRole('button', { name: 'Open source My experience' })).toBeVisible();
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const state = await mockBackend.invoke('bootstrap');
    const chatId = state.chats.find((chat) => chat.title === 'My experience').id;
    for (let index = 0; index < 40; index++) {
      await mockBackend.invoke('start_chat', { input: { chatId, content: `Later example ${index}`, providerId: 'mock', modelId: 'melly-demo', webSearch: false, folderContext: false, retry: false } });
      await mockBackend.invoke('stop_chat', { chatId });
    }
  });
  await page.getByRole('button', { name: 'Open source My experience' }).click();
  await expect(page.locator('.message.highlighted')).toContainText('In this synthetic example, I cleaned a city transit');
  await expect(page.locator('.message')).toHaveCount(22);
  await expect(page.getByRole('button', { name: 'Back to latest messages' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry response' })).toHaveCount(0);
});

test('opens a renamed citation source outside the recent chat list by its ID', async ({ page }) => {
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      const result = await original(command, args);
      if (command === 'demo_seed') {
        const source = result.chats.find((chat) => chat.title === 'My experience');
        await original('rename_chat', { chatId: source.id, title: 'Archived transit project' });
        result.chats = result.chats.filter((chat) => chat.id !== source.id);
      }
      return result;
    };
  });
  await page.getByRole('button', { name: 'Try an example' }).click();
  await page.getByRole('button', { name: 'Open source My experience' }).click();
  await expect(page.locator('.chat-title')).toHaveText('Archived transit project');
  await expect(page.locator('.message.highlighted')).toContainText('reduced average waiting time by 12%');
});

test('an empty crash-interrupted reply displays a recovery notice', async ({ page }) => {
  await page.evaluate(async () => {
    const { mockBackend } = await import(performance.getEntriesByType('resource').find((entry) => /\/src\/mock-backend\.ts(?:\?|$)/.test(entry.name))!.name);
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      const result = await original(command, args);
      if (command === 'messages') for (const message of result) if (message.role === 'assistant') {
        message.status = 'interrupted'; message.content = ''; message.sources = [];
      }
      return result;
    };
  });
  await page.getByRole('button', { name: 'Try an example' }).click();
  await expect(page.getByText('Response interrupted', { exact: true })).toBeVisible();
});

test('deleting the selected chat removes it from the recent list', async ({ page }) => {
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await expect(page.locator('.chat-title')).toHaveText('New chat');
  await page.getByRole('button', { name: 'Chat options', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Delete chat' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete chat' }).click();
  await expect(page.locator('.sidebar .recent-row')).toHaveCount(0);
  await expect(page.locator('.chat-title')).toHaveCount(0);
});
