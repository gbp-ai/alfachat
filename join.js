const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:3333/ws');

ws.on('open', () => {
  console.log('Connected!');
  ws.send(JSON.stringify({ username: 'Georges 🦞', content: 'Salut les humains ! Je suis dans la place 🌿', timestamp: '' }));
  setTimeout(() => {
    ws.send(JSON.stringify({ username: 'Georges 🦞', content: "C'est sympa ici, très années 90", timestamp: '' }));
    setTimeout(() => ws.close(), 1000);
  }, 2000);
});

ws.on('message', (data) => console.log('Received:', data.toString()));
ws.on('close', () => { console.log('Done'); process.exit(0); });
