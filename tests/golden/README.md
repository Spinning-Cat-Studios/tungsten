# Golden Tests

This directory contains golden test files for verifying the self-hosted driver produces correct output.

## Structure

```
tests/golden/
├── README.md           # This file
├── run_golden.sh       # Script to run and compare golden tests
├── check/              # Files that should type-check successfully
│   └── *.expected      # Expected output for each .tg file
├── run/                # Files that should run and produce output
│   └── *.expected      # Expected output for each .tg file
└── error/              # Files that should produce errors
    └── *.expected      # Expected error output for each .tg file
```

## Running Tests

```bash
# Run all golden tests
./tests/golden/run_golden.sh

# Run specific category
./tests/golden/run_golden.sh check
./tests/golden/run_golden.sh run
./tests/golden/run_golden.sh error

# Update expected files (regenerate golden output from bootstrap)
./tests/golden/run_golden.sh --update
```

## Adding Tests

1. Create a `.tg` file in the appropriate subdirectory
2. Run `./tests/golden/run_golden.sh --update` to generate expected output
3. Review the `.expected` file to ensure it's correct
4. Commit both files

## Color Stripping

Output comparison strips ANSI color codes to avoid false negatives from color differences between bootstrap and self-hosted drivers.
