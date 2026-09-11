import 'dotenv/config';
import express from 'express';
import cors from 'cors';
import routes from './routes.js';
import { initializeHelpers, getHealthStatus, log } from './utils/helpers.js';
import { retryFailedEvents, getGatewayStats } from './webhookGateway.js';
import fetch from 'node-fetch';
import fs from 'fs';

// ==================================================
// 🚀 EXPRESS APP INITIALIZATION
// ==================================================
const app = express();
const PORT = process.env.PORT || 3000;

// ==================================================
// 🛡️ MIDDLEWARE
// ==================================================
const allowedOrigins = process.env.ALLOWED_ORIGINS?.split(',') || ['http://localhost:3000'];

app.use(cors({
  origin: (origin, callback) => {
    if (!origin || allowedOrigins.includes(origin)) {
      callback(null, true);
    } else {
      callback(new Error('Not allowed by CORS'));
    }
  },
  methods: ['GET', 'POST', 'PUT', 'DELETE'],
  allowedHeaders: ['Content-Type', 'Authorization'],
  credentials: true,
}));

app.use(express.json({ limit: '10mb' }));

app.use((req, res, next) => {
  log(`📥 ${req.method} ${req.path} from ${req.ip}`);
  next();
});

// ==================================================
// 🛣️ ROUTES
// ==================================================
app.use('/api', routes);

app.get('/health', (req, res) => {
  const health = getHealthStatus();
  log(`🏥 Health check requested - Status: ${health.status}`);
  res.json(health);
});

app.get('/', (req, res) => {
  res.json({
    name: 'B-Pay Backend',
    version: '1.0.0',
    status: 'running',
    endpoints: {
      health: '/health',
      pay: '/api/pay',
      verify: '/api/verify',
      webhooks: '/api/webhooks/:provider',
      gatewayStats: '/gateway-stats',
      myIp: '/my-ip'
    },
  });
});

// ==================================================
// 🌐 OUTBOUND IP MONITOR
// ==================================================
const CHECK_INTERVAL = 5 * 60 * 1000; // 5 minutes
const FILE = "last-ip.txt";
let currentIP = null;

// Fetch current outbound IP
async function getOutboundIP() {
  try {
    const res = await fetch("https://api.ipify.org?format=json");
    const data = await res.json();
    return data.ip;
  } catch (err) {
    log(`Error fetching outbound IP: ${err.message}`, 'error');
    return null;
  }
}

// Check for IP changes
async function checkIP() {
  const ip = await getOutboundIP();
  if (!ip) return;

  let lastIP = null;
  if (fs.existsSync(FILE)) {
    lastIP = fs.readFileSync(FILE, "utf-8");
  }

  if (ip !== lastIP) {
    log(`🌐 Outbound IP changed to: ${ip}`, 'info');
    fs.writeFileSync(FILE, ip, 'utf-8');

    // Optional: add webhook/email notifications here
  } else {
    log(`🌐 Outbound IP unchanged: ${ip}`, 'info');
  }

  currentIP = ip;
}

// Run immediately and on interval
checkIP();
setInterval(checkIP, CHECK_INTERVAL);

// Endpoint to check current IP
app.get('/my-ip', (req, res) => {
  if (!currentIP) {
    return res.status(503).send("IP not yet fetched. Try again in a few seconds.");
  }
  res.send(`Current Outbound IP: ${currentIP}`);
});

// ==================================================
// 🌐 WEBHOOK GATEWAY — RETRY SWEEP (Task 41)
// ==================================================
// Re-attempts any Korapay webhook event still sitting in 'failed'
// status (downstream tenant app was briefly unreachable, etc.), on the
// same interval-based pattern as the outbound-IP monitor above rather
// than a queue/cron system this repo has no infra for yet. See
// webhookGateway.js's own file header for the full design — this
// backend is stateless by design (no database, confirmed architecture
// decision: each tenant app owns its own durable recording via its own
// edge function), so this in-memory retry window is the correct final
// shape here, not a stopgap.
const GATEWAY_RETRY_INTERVAL = 60 * 1000; // 1 minute
setInterval(() => {
  retryFailedEvents().catch((err) => log(`Gateway retry sweep threw: ${err.message}`, 'error'));
}, GATEWAY_RETRY_INTERVAL);

// Lightweight visibility into the gateway's in-memory state — counts
// only, never raw event payloads (those can carry customer emails/
// amounts, no reason to expose them on an unauthenticated endpoint).
app.get('/gateway-stats', (req, res) => {
  res.json(getGatewayStats());
});

// ==================================================
// ❌ ERROR HANDLING
// ==================================================
app.use((req, res) => {
  log(`⚠️ 404 - Route not found: ${req.method} ${req.path}`);
  res.status(404).json({ status: false, message: 'Endpoint not found' });
});

app.use((err, req, res, next) => {
  log(`❌ Global Error: ${err.message}`, 'error');
  res.status(err.status || 500).json({ status: false, message: err.message || 'Internal server error' });
});

// ==================================================
// 🎯 SERVER STARTUP
// ==================================================
initializeHelpers();

app.listen(PORT, () => {
  log(`✅ B-Pay Backend is running`, 'info');
  log(`🌍 Port: ${PORT}`, 'info');
  log(`🔧 Environment: ${process.env.NODE_ENV || 'development'}`, 'info');
  log(`📡 Health Check: http://localhost:${PORT}/health`, 'info');
  log(`🌐 Outbound IP monitor active. Visit /my-ip to check current IP`, 'info');
});

// ==================================================
// 🛑 GRACEFUL SHUTDOWN
// ==================================================
process.on('SIGTERM', () => {
  log('🛑 SIGTERM received. Shutting down gracefully...', 'warn');
  process.exit(0);
});

process.on('SIGINT', () => {
  log('🛑 SIGINT received. Shutting down gracefully...', 'warn');
  process.exit(0);
});

export default app;