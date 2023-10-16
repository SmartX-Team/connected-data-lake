import os

from openark import OpenArk
from pandas import json_normalize, read_csv


def load_data(output_file: str):
    # Init OpenARK client
    ark = OpenArk()

    # Get model as DeltaTable
    m = ark.get_model('performance-test-src')
    df = m.to_delta().to_pandas()

    # Flatten JSON data
    df_timestamp = df['timestamp']
    df_value = json_normalize([
        value['data']
        for value in df['value']
    ])

    df = df_value
    df['timestamp'] = df_timestamp

    # Write as .csv to be cached
    df.to_csv(output_file)


def load_csv(filename: str):
    if not os.path.exists(filename):
        load_data(filename)
    return read_csv(filename)


if __name__ == '__main__':
    csv_data = load_csv('./output.csv')
