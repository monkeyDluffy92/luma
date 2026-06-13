# Project Summary: Luma

**Luma** is a dedicated focus timer application built for macOS, designed to help you tackle complex subjects through structured 90-minute study sessions.

## What You Made
You have built a fully native macOS application using **Tauri (Rust)**. Unlike heavy Electron apps (like VS Code or Slack) which can use hundreds of megabytes of RAM, Luma runs on a lightweight Rust backend, making it incredibly fast and efficient.

It features a custom "Frameless Dark UI" that eschews standard window bars for a sleek, immersive look. It lives in your menu bar (System Tray) and stays out of your way until you need it, launching automatically when you start your computer.

## Why It's Useful for Learners

### 1. It Enforces "Active Intent"
Most timers let you just click "Start". Luma forces you to type **"What are you studying?"** before the timer begins. This subtle friction acts as a psychological contract—you aren't just passing time; you are working on *Calculus* or *System Design*.

### 2. It Handles Real Life (Sleep/Wake)
Learners often close their laptops to go to a lecture or grab coffee. Luma detects when your computer goes to sleep. When you wake it up, it intelligently asks: *"You were away for 15 minutes. Did you take a break, or were you working offline?"* This keeps your study data accurate without you needing to micromanage the timer.

### 3. It Gamifies Consistency
The "Streak" counter on the dashboard is designed to build a daily habit. By tracking "Completed Sessions" rather than just hours, it rewards *finishing* what you started, not just staring at the screen.

### 4. It Respects Your Focus
The UI is dark, minimal, and silent. It doesn't have notifications, social feeds, or clutter. It respects that your attention is the most valuable resource you have.
