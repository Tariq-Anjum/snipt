## Snipt 1.1.1

**Fixes**
- Loyalty "money" (Power Cash, SuperCoins, NeuCoins, reward/loyalty points, bonus cash) is no longer counted as a credit, and such messages go to Promotions.
- Inbox rows fit their text (no clipped third line or overlapping meta line).
- Startup: if the app core does not answer, Snipt retries and then shows a Retry button instead of staying locked.
- MMS: messages with more parts than Snipt can show now say so; temporary download files are cleaned up after failures.

Existing messages are re-checked once after updating.

**Install:** download `Snipt-arm64.apk` on your phone, or on Linux run
`curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash`.
