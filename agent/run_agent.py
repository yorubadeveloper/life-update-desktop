"""PyInstaller entrypoint - analyzes a script, not an installed console-script."""

from life_update_agent.cli import main

if __name__ == "__main__":
    main()
