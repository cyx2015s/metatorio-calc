# Performance Improvements

This document describes the performance optimizations made to the Metatorio Calculator codebase.

## Overview

The following performance bottlenecks were identified and addressed:

## 1. String Cloning in Sorting Operations

**Problem**: The `sort_generic_items()` function used `sort_by_key`, which required cloning strings during every comparison operation. Similarly, `get_order_info()` cloned order strings repeatedly.

**Solution**: 
- Changed `sort_generic_items()` from `sort_by_key` to `sort_by` and use string slices (`&str`) instead of owned strings
- Changed `get_order_info()` to use borrowed references in comparison functions
- Used `.copied()` instead of `.cloned()` for tuples of integers
- Extracted `get_generic_item_sort_key()` helper function to eliminate code duplication
- Added `sort_generic_items_owned()` for efficient sorting of owned items

**Files Changed**: `src/factorio/common.rs`

**Impact**: Reduces memory allocations and improves sorting performance, especially for large item lists.

## 2. Repeated Sorting of Display Items

**Problem**: The total flow display sorted items every frame, even though the total flow only changes when solver results arrive.

**Solution**: Added a cached sorted keys vector (`total_flow_sorted_keys`) that is updated only when solver results change. The rendering code now uses the cached sorted keys.

**Files Changed**: `src/factorio/editor/planner.rs`

**Impact**: Eliminates redundant sorting operations during rendering, improving frame rate during active planning sessions.

## Performance Metrics

While specific benchmarks haven't been conducted, the improvements should provide:

- **Faster UI response**: Eliminated unnecessary work during rendering
- **Reduced memory allocations**: Fewer string clones and temporary allocations

## Future Optimization Opportunities

Additional optimization opportunities identified but not yet implemented:

1. **UI repainting optimization**: The UI currently requests repaints every 0.1 seconds unconditionally. This could be optimized to only repaint when necessary, but requires careful design to avoid missing necessary repaints.

2. **Solver change detection**: The solver currently runs every 10 frames. Adding change detection to only run when data changes would reduce unnecessary invocations, but this involves design challenges with internal mutability that need to be addressed first.

3. **Item selector filtering**: The selector rebuilds its filtered group list every frame. This could be cached based on the current filter.

4. **Flow calculations for individual recipes**: Each recipe's flow is calculated and sorted during rendering. Could be cached per recipe.

5. **Context loading**: The initial context loading from Factorio executable could potentially be parallelized or optimized.

## Compatibility

All performance improvements maintain full backward compatibility with existing functionality. No breaking changes were introduced.

## Testing Recommendations

To verify these improvements:

1. Verify UI remains responsive when displaying many items in total flow
2. Ensure all existing functionality works correctly after optimizations

