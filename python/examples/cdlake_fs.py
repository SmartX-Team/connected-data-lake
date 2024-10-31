import cdlake


def main():
    cdl = cdlake.Cdl()
    fs = cdl.open('s3a://dataset-a/')
    fs.copy_to('./dst')


if __name__ == '__main__':
    main()
