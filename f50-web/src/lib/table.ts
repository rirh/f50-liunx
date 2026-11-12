import {
  type ColumnDef,
  type RowData,
  columnFilteringFeature,
  createColumnHelper,
  createFilteredRowModel,
  createSortedRowModel,
  filterFn_includesString,
  globalFilteringFeature,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  sortFn_text,
  tableFeatures,
} from "@tanstack/react-table";

/** The features every table of the console uses: client-side sorting and a global text filter. */
export const tableFeaturesSet = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: {
    alphanumeric: sortFn_alphanumeric,
    text: sortFn_text,
    basic: sortFn_basic,
  },
  columnFilteringFeature,
  globalFilteringFeature,
  filteredRowModel: createFilteredRowModel(),
  filterFns: { includesString: filterFn_includesString },
});

type Features = typeof tableFeaturesSet;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type TableColumn<T extends RowData> = ColumnDef<Features, T, any>;

/** A column helper bound to the console's table features. */
export const columnHelper = <T extends RowData>() =>
  createColumnHelper<Features, T>();
