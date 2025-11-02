import polars as pl

if __name__ == "__main__":
    df = pl.DataFrame({"a": [1, 2, 3], "b": ["a", "b", "c"]})
    print(df)