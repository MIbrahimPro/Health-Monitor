import cv2
import time
import os

OUTPUT_DIR = "tests"
OUTPUT_FILE = os.path.join(OUTPUT_DIR, "test_video.avi")
DURATION_SECONDS = 180  # 3 minutes

if not os.path.exists(OUTPUT_DIR):
    os.makedirs(OUTPUT_DIR)

print(f"Opening camera to record for {DURATION_SECONDS} seconds...")

cap = cv2.VideoCapture(0)

if not cap.isOpened():
    print("Error: Could not open camera.")
    exit(1)

# Get default resolution
frame_width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
frame_height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
fps = cap.get(cv2.CAP_PROP_FPS)

# Sometimes webcam fps is returned as 0 or wildly inaccurate, assume 30 for writing if so
if fps == 0.0 or fps < 0:
    fps = 30.0

print(f"Resolution: {frame_width}x{frame_height} @ {fps} FPS")

# Define the codec and create VideoWriter object
# XVID or MJPG are common and safe
fourcc = cv2.VideoWriter_fourcc(*'XVID')
out = cv2.VideoWriter(OUTPUT_FILE, fourcc, fps, (frame_width, frame_height))

start_time = time.time()
frame_count = 0

print("Recording started. Please look at the camera...")

while (time.time() - start_time) < DURATION_SECONDS:
    ret, frame = cap.read()
    if ret:
        out.write(frame)
        frame_count += 1
        
        # Print progress every 10 seconds
        elapsed = int(time.time() - start_time)
        if elapsed > 0 and elapsed % 10 == 0 and frame_count % int(fps) == 0:
            print(f"Recording... {elapsed} / {DURATION_SECONDS} seconds elapsed.")
    else:
        print("Warning: Dropped frame.")
        break

# Release everything
cap.release()
out.release()
print(f"Recording finished! Saved to {OUTPUT_FILE}. Total frames: {frame_count}")
