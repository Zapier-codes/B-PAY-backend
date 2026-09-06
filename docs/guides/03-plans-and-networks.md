# Plans and networks

`GET /plans` — optionally filtered by:
- `network`: `MTN` | `AIRTEL` | `GLO` | `9MOBILE` (Nigeria only — see
  `08-adding-a-country-rail.md` before adding another country's
  networks to this list)
- `category`: `data` | `airtime`

Each plan in the response:

| Field       | Type   | Notes                          |
|-------------|--------|---------------------------------|
| `id`        | string | Internal plan identifier        |
| `plan_id`   | number | Numeric plan reference used when purchasing |
| `network`   | string | One of the four NG networks above |
| `plan_type` | string | CONFIRM the full value set — only one example (`"string"`) was in the capture |
| `plan_name` | string | Display name, e.g. "1GB - 30 days" |
| `price`     | number | Assumed NGN — no explicit currency field yet, see country-rail guide point 5 |
| `validity`  | string | e.g. "30 days" |

`plan_id` from this list is what you pass as `planId` when calling
`POST /purchase/data`.
