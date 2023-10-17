import os

from openark import OpenArk
from pandas import DataFrame, json_normalize, read_csv, to_datetime, to_timedelta


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


def inspect_data(df: DataFrame, output_file: str):
    # Fill null values
    df['payload_size'].fillna(0, inplace=True)

    # Group by metrics set
    group = df.groupby(['data_size', 'messenger_type', 'payload_size'])
    group_first = group.first()
    group_last = group.last()

    # Calculate duration
    df = group_last
    df['duration'] = (
        to_datetime(group_last['timestamp'])
        - to_datetime(group_first['timestamp'])
        + to_timedelta('1s')
    ).dt.total_seconds()

    # Calculate speed
    df['speed'] = 8 * df['total_sent_bytes'] / df['duration']
    df['speed_payload'] = 8 * df['total_sent_payload_bytes'] / df['duration']

    # Write as .csv to be cached
    df.to_csv(output_file)


def load_csv(filename: str, f):
    if not os.path.exists(filename):
        load_data(filename)
    return f(filename)


if __name__ == '__main__':
    inspected_data = load_csv(
        './inspected.csv',
        lambda filename: inspect_data(
            load_csv('./raw.csv', read_csv),
            filename,
        )
    )
