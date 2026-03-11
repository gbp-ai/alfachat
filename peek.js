const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:3333/ws');
const messages = [];

ws.on('open', () => setTimeout(() => ws.close(), 500));
ws.on('message', (data) => messages.push(JSON.parse(data.toString())));
ws.on('close', () => {
  console.log('=== AlfaChat History ===');
  messages.forEach(m => console.log(`[${m.timestamp}] <${m.username}> ${m.content}`));
  process.exit(0);
});
