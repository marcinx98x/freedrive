package sqlite

import (
	"context"
	"database/sql"
	"time"
)

// BandwidthRepo implements repository.BandwidthRepository.
type BandwidthRepo struct {
	writer *sql.DB
	reader *sql.DB
}

func NewBandwidthRepo(db *DB) *BandwidthRepo {
	return &BandwidthRepo{writer: db.Writer, reader: db.Reader}
}

func currentYearMonthUTC() string {
	return time.Now().UTC().Format("2006-01")
}

func (r *BandwidthRepo) AddUpload(ctx context.Context, userID string, bytes int64) error {
	return r.add(ctx, userID, bytes, 0)
}

func (r *BandwidthRepo) AddDownload(ctx context.Context, userID string, bytes int64) error {
	return r.add(ctx, userID, 0, bytes)
}

func (r *BandwidthRepo) add(ctx context.Context, userID string, upload, download int64) error {
	if userID == "" || (upload <= 0 && download <= 0) {
		return nil
	}
	ym := currentYearMonthUTC()
	_, err := r.writer.ExecContext(ctx, `
		INSERT INTO bandwidth_monthly (user_id, year_month, upload_bytes, download_bytes)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(user_id, year_month) DO UPDATE SET
			upload_bytes = upload_bytes + excluded.upload_bytes,
			download_bytes = download_bytes + excluded.download_bytes
	`, userID, ym, upload, download)
	return err
}

func (r *BandwidthRepo) SumMonth(ctx context.Context, yearMonth string) (uploadBytes, downloadBytes int64, err error) {
	if yearMonth == "" {
		yearMonth = currentYearMonthUTC()
	}
	err = r.reader.QueryRowContext(ctx, `
		SELECT COALESCE(SUM(upload_bytes), 0), COALESCE(SUM(download_bytes), 0)
		FROM bandwidth_monthly
		WHERE year_month = ?
	`, yearMonth).Scan(&uploadBytes, &downloadBytes)
	return uploadBytes, downloadBytes, err
}
