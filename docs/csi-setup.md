# Wi-Fi CSI Setup

To use the Wi-Fi Channel State Information (CSI) features in Aegis, you need specific hardware and patched firmware.

## Supported Hardware
- Intel 5300 (using Linux 802.11n CSI Tool)
- Certain Broadcom chips (using Nexmon CSI)

## Nexmon Setup
1. Use a supported Broadcom device (e.g., Raspberry Pi 3B+/4B).
2. Install the Nexmon firmware patches.
3. Configure `mcpd` to forward UDP packets to localhost:5500.

The `NexmonSource` in `aegis-core` expects UDP packets on port 5500 containing the raw CSI matrix.
