package sqlite

import (
	"context"
	"database/sql"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/google/uuid"
)

// CommentRepo implements repository.CommentRepository.
type CommentRepo struct {
	writer *sql.DB
	reader *sql.DB
}

func NewCommentRepo(db *DB) *CommentRepo {
	return &CommentRepo{writer: db.Writer, reader: db.Reader}
}

func (r *CommentRepo) Create(ctx context.Context, comment *domain.Comment) error {
	if comment.ID == "" {
		comment.ID = uuid.New().String()
	}
	now := time.Now()
	comment.CreatedAt = now
	comment.UpdatedAt = now

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO comments (id, file_id, user_id, content, parent_id, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		comment.ID, comment.FileID, comment.UserID, comment.Content, comment.ParentID, comment.CreatedAt, comment.UpdatedAt)
	return err
}

func (r *CommentRepo) GetByFileID(ctx context.Context, fileID string) ([]domain.Comment, error) {
	rows, err := r.reader.QueryContext(ctx,
		`SELECT c.id, c.file_id, c.user_id, u.username, c.content, c.parent_id, c.created_at, c.updated_at
		 FROM comments c JOIN users u ON c.user_id = u.id
		 WHERE c.file_id = ? ORDER BY c.created_at ASC`, fileID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var comments []domain.Comment
	for rows.Next() {
		var c domain.Comment
		if err := rows.Scan(&c.ID, &c.FileID, &c.UserID, &c.Username, &c.Content, &c.ParentID, &c.CreatedAt, &c.UpdatedAt); err != nil {
			return nil, err
		}
		comments = append(comments, c)
	}
	return comments, nil
}

func (r *CommentRepo) Delete(ctx context.Context, id string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM comments WHERE id = ?", id)
	return err
}

// ActivityRepo implements repository.ActivityRepository.
type ActivityRepo struct {
	writer *sql.DB
	reader *sql.DB
}

func NewActivityRepo(db *DB) *ActivityRepo {
	return &ActivityRepo{writer: db.Writer, reader: db.Reader}
}

func (r *ActivityRepo) Create(ctx context.Context, log *domain.ActivityLog) error {
	if log.ID == "" {
		log.ID = uuid.New().String()
	}
	log.CreatedAt = time.Now()

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO activity_log (id, user_id, action, target_type, target_id, target_name, metadata, ip_address, created_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		log.ID, log.UserID, log.Action, log.TargetType, log.TargetID,
		log.TargetName, log.Metadata, log.IPAddress, log.CreatedAt)
	return err
}

func (r *ActivityRepo) List(ctx context.Context, userID string, page, pageSize int) ([]domain.ActivityLog, int, error) {
	if page < 1 {
		page = 1
	}
	if pageSize < 1 || pageSize > 100 {
		pageSize = 20
	}

	var total int
	if err := r.reader.QueryRowContext(ctx,
		"SELECT COUNT(*) FROM activity_log WHERE user_id = ?", userID).Scan(&total); err != nil {
		return nil, 0, err
	}

	rows, err := r.reader.QueryContext(ctx,
		`SELECT a.id, a.user_id, u.username, a.action, a.target_type, a.target_id, a.target_name, a.metadata, a.ip_address, a.created_at
		 FROM activity_log a JOIN users u ON a.user_id = u.id
		 WHERE a.user_id = ? ORDER BY a.created_at DESC LIMIT ? OFFSET ?`,
		userID, pageSize, (page-1)*pageSize)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var logs []domain.ActivityLog
	for rows.Next() {
		var l domain.ActivityLog
		if err := rows.Scan(&l.ID, &l.UserID, &l.Username, &l.Action, &l.TargetType, &l.TargetID,
			&l.TargetName, &l.Metadata, &l.IPAddress, &l.CreatedAt); err != nil {
			return nil, 0, err
		}
		logs = append(logs, l)
	}
	return logs, total, nil
}

func (r *ActivityRepo) ListAll(ctx context.Context, page, pageSize int) ([]domain.ActivityLog, int, error) {
	if page < 1 {
		page = 1
	}
	if pageSize < 1 || pageSize > 100 {
		pageSize = 20
	}

	var total int
	if err := r.reader.QueryRowContext(ctx,
		"SELECT COUNT(*) FROM activity_log").Scan(&total); err != nil {
		return nil, 0, err
	}

	rows, err := r.reader.QueryContext(ctx,
		`SELECT a.id, a.user_id, u.username, a.action, a.target_type, a.target_id, a.target_name, a.metadata, a.ip_address, a.created_at
		 FROM activity_log a JOIN users u ON a.user_id = u.id
		 ORDER BY a.created_at DESC LIMIT ? OFFSET ?`,
		pageSize, (page-1)*pageSize)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var logs []domain.ActivityLog
	for rows.Next() {
		var l domain.ActivityLog
		if err := rows.Scan(&l.ID, &l.UserID, &l.Username, &l.Action, &l.TargetType, &l.TargetID,
			&l.TargetName, &l.Metadata, &l.IPAddress, &l.CreatedAt); err != nil {
			return nil, 0, err
		}
		logs = append(logs, l)
	}
	return logs, total, nil
}
